use crate::host::{CookieFunctions, HostContext, RequestFunctions, ResponseFunctions};
use anyhow::{anyhow, bail, Context, Result};
use async_std::{net::TcpListener, task};
use async_trait::async_trait;
use futures::{channel::oneshot, select, stream::StreamExt, FutureExt};
use futures_timer::Delay;
use http_types::StatusCode;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use stop_token::{StopSource, StopToken};
use wasmtime::{Config, Engine, Extern, Instance, InterruptHandle, Linker, Module, Store, ValType};
use wasmtime_functions_metadata::{FunctionTrigger, Metadata};
use wasmtime_wasi::{Wasi, WasiCtxBuilder};

const FUNCTION_TIMEOUT_SECS: u64 = 300;

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
    env: HashMap<String, String>,
    inherit_stdout: bool,
}

impl StateInner {
    pub fn instantiate(
        &self,
        request: Request,
        body: Vec<u8>,
    ) -> Result<(HostContext, Instance, Option<InterruptHandle>)> {
        let ctx = HostContext::new(request, body);
        let store = Store::new(&self.module.engine());

        let mut builder = WasiCtxBuilder::new();

        builder.envs(&self.env);

        if self.inherit_stdout {
            builder.inherit_stdout().inherit_stderr();
        }

        let mut linker = Linker::new(&store);

        Wasi::new(&store, builder.build()?).add_to_linker(&mut linker)?;
        RequestFunctions::new(&store, ctx.clone()).add_to_linker(&mut linker)?;
        ResponseFunctions::new(&store, ctx.clone()).add_to_linker(&mut linker)?;
        CookieFunctions::new(&store, ctx.clone()).add_to_linker(&mut linker)?;

        let instance = linker.instantiate(&self.module)?;

        // Call the start function to initialize any environment variables
        if let Some(start) = instance.get_export("_start") {
            if let Some(func) = start.into_func() {
                log::debug!("Calling instance start function.");
                func.call(&[])
                    .context("the module start function trapped")?;
            }
        }

        Ok((ctx, instance, store.interrupt_handle().ok()))
    }
}

#[derive(Clone)]
struct Endpoint {
    function: Arc<String>,
}

impl Endpoint {
    async fn invoke_function(&self, mut req: tide::Request<State>) -> Result<tide::Response> {
        let body = req.body_bytes().await.map_err(|e| anyhow::anyhow!(e))?;
        let function = self.function.clone();

        let (tx, rx) = oneshot::channel();

        let mut timeout = Delay::new(Duration::from_secs(FUNCTION_TIMEOUT_SECS)).fuse();

        let mut invocation = task::spawn_blocking(move || -> Result<tide::Response> {
            let state = req.state().inner.clone();
            let (host_ctx, instance, interrupt) = state.instantiate(req, body)?;

            tx.send(interrupt)
                .map_err(|_| anyhow!("failed to send interrupt handle."))?;

            let func = match instance.get_export(&function) {
                Some(Extern::Func(f)) => f,
                _ => bail!("function '{}' was not found.", function),
            };

            let ty = func.ty();
            if !ty.params().is_empty() || ty.results() != [ValType::I32] {
                bail!("function '{}' has an incorrect signature.", function);
            }

            log::info!("invoking function '{}'.", function);

            let res = func
                .call(&[])
                .map(|r| r[0].unwrap_i32())
                .map_err(|e| anyhow!("function call to '{}' trapped: {}", function, e))?
                .into();

            Ok(host_ctx
                .take_response(res)
                .ok_or_else(|| anyhow!("function did not return a HTTP response"))?)
        })
        .fuse();

        select! {
            res = invocation => res,
            _ = timeout => {
                if let Some(interrupt) = rx.await? {
                    interrupt.interrupt();
                }

                Err(anyhow!("function invocation for '{}' timed out after {} seconds.", self.function, FUNCTION_TIMEOUT_SECS))
            },
        }
    }
}

#[async_trait]
impl tide::Endpoint<State> for Endpoint {
    async fn call(&self, req: tide::Request<State>) -> tide::Result {
        self.invoke_function(req).await.map_err(|e| {
            log::error!("{}", e);
            tide::Error::from_str(StatusCode::InternalServerError, e.to_string())
        })
    }
}

/// The Wasmtime Functions HTTP server.
///
/// This server is used to host the given WebAssembly module and route requests to Wasmtime functions.
pub struct Server {
    port: u16,
    _source: StopSource,
}

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

        let mut env = HashMap::new();
        for name in metadata.vars {
            let value = environment.var(&name)?;
            env.insert(name, value);
        }

        let engine = Engine::new(Config::default().debug_info(debug_info).interruptable(true));

        let module = Module::new(&engine, module)?;

        let mut app = tide::with_state(State {
            inner: Arc::new(StateInner {
                module,
                env,
                inherit_stdout,
            }),
        });

        for function in metadata.functions {
            match &function.trigger {
                FunctionTrigger::Http { path, methods } => {
                    let mut route = app.at(path);

                    let endpoint = Endpoint {
                        function: Arc::new(function.name.clone()),
                    };

                    if methods.is_empty() {
                        log::info!(
                            "adding route for function '{}' at '{}'.",
                            function.name,
                            path,
                        );
                        route.all(endpoint);
                    } else {
                        for method in methods {
                            log::info!(
                                "adding route for function '{}' at '{}' ({}).",
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

        // Unfortunately there's no way to bind a tide `Server` prior to listening
        // Thus we cannot determine the bound port from tide's API
        // Therefore implement a TcpListener and run loop here
        let listener = TcpListener::bind(addr.into()).await?;
        let port = listener.local_addr()?.port();

        let source = StopSource::new();

        task::spawn(Self::run(listener, app, source.stop_token()));

        Ok(Self {
            port,
            _source: source,
        })
    }

    /// Gets the port used by the server.
    pub fn port(&self) -> u16 {
        self.port
    }

    async fn run(listener: TcpListener, app: tide::Server<State>, token: StopToken) -> Result<()> {
        let mut incoming = token.stop_stream(listener.incoming());

        while let Some(stream) = incoming.next().await {
            let stream = stream?;
            let local_addr = stream.local_addr().ok();
            let peer_addr = stream.peer_addr().ok();
            let app = app.clone();

            task::spawn(async move {
                if let Err(e) = async_h1::accept(stream, |mut req| async {
                    req.set_local_addr(local_addr);
                    req.set_peer_addr(peer_addr);

                    Ok(app
                        .respond(req)
                        .await
                        .map_err(|_| async_std::io::Error::from(async_std::io::ErrorKind::Other))?)
                })
                .await
                {
                    log::error!("error accepting connection: {}", e);
                }
            });
        }

        Ok(())
    }
}
