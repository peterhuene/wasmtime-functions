use crate::host::{Functions, HostContext};
use anyhow::{anyhow, bail, Context as _, Result};
use async_trait::async_trait;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_functions_metadata::{FunctionTrigger, Metadata};
use wasmtime_wasi::{sync::WasiCtxBuilder, Wasi};

const FUNCTION_TIMEOUT_SECS: u64 = 60;

/// Provides environment variables to the runtime server.
pub trait EnvironmentProvider {
    /// Gets the environment variable of the given name.
    fn var(&self, name: &str) -> Result<String>;
}

pub type Request = tide::Request<State>;

#[derive(Clone)]
pub struct State {
    inner: Arc<StateInner>,
}

struct StateInner {
    module: Module,
    env: Vec<(String, String)>,
    inherit_stdout: bool,
}

impl StateInner {
    pub async fn instantiate(
        &self,
        request: Request,
        body: Vec<u8>,
    ) -> Result<(Rc<RefCell<HostContext>>, Instance)> {
        let store = Store::new(&self.module.engine());

        // Yield to the host every 10000-or-so Wasm instructions
        store.out_of_fuel_async_yield(u32::MAX, 10000);

        let ctx = Rc::new(RefCell::new(HostContext::new(request, body)));

        let mut wasi_ctx = WasiCtxBuilder::new();

        if self.inherit_stdout {
            wasi_ctx = wasi_ctx.inherit_stdout().inherit_stderr();
        }

        wasi_ctx = wasi_ctx.envs(&self.env)?;

        assert!(Wasi::set_context(&store, wasi_ctx.build()?).is_ok());
        assert!(store.set(ctx.clone()).is_ok());

        let linker = Linker::new(&store);
        let instance = linker.instantiate_async(&self.module).await?;

        // Call the start function to initialize any environment variables
        if let Ok(f) = instance.get_typed_func::<(), ()>("_start") {
            f.call_async(()).await?;
        }

        Ok((ctx, instance))
    }
}

struct UnsafeSend<T>(T);

// Note the `where` clause which specifically ensures that the output of the
// future to be `Send` is required. We specifically don't require `T` to be
// `Send` since that's the whole point of this function, but we require that
// everything used to construct `T` is `Send` below.
unsafe impl<T> Send for UnsafeSend<T>
where
    T: Future,
    T::Output: Send,
{
}

impl<T: Future> Future for UnsafeSend<T> {
    type Output = T::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T::Output> {
        // Note that this `unsafe` is unrelated to `Send`, it only has to do
        // with "pin projection" and should be safe since it's all we do
        // with the `Pin`.
        unsafe { self.map_unchecked_mut(|p| &mut p.0).poll(cx) }
    }
}

#[derive(Clone)]
struct Endpoint {
    function: Arc<String>,
}

impl Endpoint {
    fn invoke_function(&self, req: tide::Request<State>) -> impl Future<Output = tide::Result> {
        // Safety: as all of the Wasmtime objects (Store, Instance, TypedFunc, etc) are owned
        // by this future and not shared with any other thread, this is safe except for
        // any use of thread-local storage. Care must be used here to avoid any libraries that
        // might not except the thread running the future to change between polls.
        UnsafeSend(Self::_invoke_function(self.function.clone(), req))
    }

    async fn _invoke_function(
        function: Arc<String>,
        mut req: tide::Request<State>,
    ) -> tide::Result {
        // TODO: move this into an async host function
        let body = req.body_bytes().await.map_err(|e| anyhow::anyhow!(e))?;
        let state = req.state().inner.clone();
        let (host_ctx, instance) = state.instantiate(req, body).await?;

        let f = instance.get_typed_func::<(), i32>(&function)?;

        log::info!("Invoking function '{}'.", function);

        let res = f
            .call_async(())
            .await
            .with_context(|| format!("call to function '{}' trapped", function))?;

        let ctx = host_ctx.borrow();

        ctx.take_response(res.into())
            .ok_or_else(|| tide::Error::from(anyhow!("function did not return a HTTP response")))
    }
}

#[async_trait]
impl tide::Endpoint<State> for Endpoint {
    async fn call(&self, req: tide::Request<State>) -> tide::Result {
        use async_std::prelude::FutureExt;

        self.invoke_function(req)
            .timeout(std::time::Duration::from_secs(FUNCTION_TIMEOUT_SECS))
            .await?
    }
}

/// The Wasmtime Functions HTTP server.
///
/// This server is used to host the given WebAssembly module and route requests to Wasmtime functions.
pub struct Server(Box<dyn tide::listener::Listener<State>>);

impl Server {
    /// Creates a runtime server.
    pub async fn new<A: Into<SocketAddr>>(
        addr: A,
        module: &[u8],
        environment: &dyn EnvironmentProvider,
        debug_info: bool,
        inherit_stdout: bool,
    ) -> Result<Self> {
        let metadata = Metadata::from_module_bytes(&module)?;

        if metadata.functions.is_empty() {
            bail!("module contains no Wasmtime functions");
        }

        let mut env = Vec::new();
        for name in metadata.vars {
            let value = environment.var(&name)?;
            env.push((name, value));
        }

        let mut config = Config::default();

        config.allocation_strategy(wasmtime::InstanceAllocationStrategy::pooling());
        config.debug_info(debug_info);
        config.consume_fuel(true);
        config.async_support(true);

        Wasi::add_to_config(&mut config);
        Functions::add_to_config(&mut config);

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, module)?;

        let mut app = tide::with_state(State {
            inner: Arc::new(StateInner {
                module,
                env,
                inherit_stdout,
            }),
        });

        app.with(crate::log::LogMiddleware);

        for function in metadata.functions {
            match &function.trigger {
                FunctionTrigger::Http { path, methods } => {
                    let mut route = app.at(path);

                    let endpoint = Endpoint {
                        function: Arc::new(function.name.clone()),
                    };

                    if methods.is_empty() {
                        log::info!(
                            "Adding route for function '{}' at '{}'.",
                            function.name,
                            path,
                        );
                        route.all(endpoint);
                    } else {
                        for method in methods {
                            log::info!(
                                "Adding route for function '{}' at '{}' ({}).",
                                function.name,
                                path,
                                method
                            );
                            http_types::Method::try_from(method.as_ref())
                                .map(|m| route.method(m, endpoint.clone()))
                                .ok();
                        }
                    }
                }
            }
        }

        Ok(Self(Box::new(app.bind(addr.into()).await?)))
    }

    /// Accepts and processes incoming connections.
    pub async fn accept(&mut self) -> Result<()> {
        self.0.accept().await?;
        Ok(())
    }
}

impl fmt::Display for Server {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0.info().first().map(|i| i.connection()).unwrap_or("")
        )
    }
}
