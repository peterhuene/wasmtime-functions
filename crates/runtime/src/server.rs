use crate::host::Context;
use anyhow::{anyhow, bail, Context as _, Result};
use async_trait::async_trait;
use std::convert::TryFrom;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};
use wasmtime_functions_metadata::{FunctionTrigger, Metadata};
use wasmtime_wasi::sync::WasiCtxBuilder;

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
    linker: Linker<Context>,
    env: Vec<(String, String)>,
    inherit_stdout: bool,
}

impl StateInner {
    pub async fn instantiate(
        &self,
        request: Request,
        body: Vec<u8>,
    ) -> Result<(Store<Context>, Instance)> {
        let mut wasi_ctx = WasiCtxBuilder::new();

        if self.inherit_stdout {
            wasi_ctx = wasi_ctx.inherit_stdout().inherit_stderr();
        }

        wasi_ctx = wasi_ctx.envs(&self.env)?;

        let mut store = Store::new(
            &self.module.engine(),
            Context::new(request, body, wasi_ctx.build()),
        );
        store.out_of_fuel_async_yield(u64::MAX, 10000);

        let instance = self
            .linker
            .instantiate_async(&mut store, &self.module)
            .await?;

        Ok((store, instance))
    }
}

#[derive(Clone)]
struct Endpoint {
    function: Arc<String>,
}

impl Endpoint {
    async fn invoke_function(&self, mut req: tide::Request<State>) -> tide::Result {
        // TODO: move this into an async host function
        let body = req.body_bytes().await.map_err(|e| anyhow::anyhow!(e))?;
        let state = req.state().inner.clone();
        let (mut store, instance) = state.instantiate(req, body).await?;

        let entry = instance.get_typed_func::<u32, u32, _>(&mut store, &self.function)?;

        let req = store.data().request_handle();

        log::info!("Invoking function '{}'.", self.function);

        let res = entry
            .call_async(&mut store, req)
            .await
            .with_context(|| format!("call to function '{}' trapped", self.function))?;

        store
            .data()
            .take_response(res)
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

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, module)?;

        let mut linker = Linker::new(&engine);
        Context::add_to_linker(&mut linker)?;

        let mut app = tide::with_state(State {
            inner: Arc::new(StateInner {
                module,
                linker,
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
