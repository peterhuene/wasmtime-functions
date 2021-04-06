use anyhow::{bail, Result};
use async_ctrlc::CtrlC;
use async_std::prelude::FutureExt;
use env_logger::builder;
use rpassword::read_password_from_tty;
use std::net::SocketAddr;
use std::path::PathBuf;
use structopt::StructOpt;
use wasmtime_functions_runtime::Server;

fn parse_env_var(s: &str) -> Result<(String, String)> {
    let parts: Vec<_> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        bail!("must be of the form `key=value`");
    }
    Ok((parts[0].to_owned(), parts[1].to_owned()))
}

struct EnvironmentProvider(Vec<(String, String)>);

impl wasmtime_functions_runtime::EnvironmentProvider for EnvironmentProvider {
    fn var(&self, name: &str) -> Result<String> {
        Ok(
            if let Some((_, v)) = self.0.iter().find(|(n, _)| n == name) {
                v.clone()
            } else if let Ok(value) = std::env::var(&name) {
                value
            } else {
                read_password_from_tty(Some(&format!(
                    "enter the value for environment variable '{}': ",
                    name
                )))?
            },
        )
    }
}

#[derive(StructOpt)]
pub struct Options {
    /// The path to the WebAssembly module to run.
    pub module: String,

    /// The listen address for the application.
    #[structopt(long, default_value = "127.0.0.1:0")]
    pub addr: SocketAddr,

    /// Enable debug information for the application.
    #[structopt(short = "g", long)]
    pub debug_info: bool,

    /// Override an application environment variable value.
    #[structopt(long = "env", short, number_of_values = 1, value_name = "NAME=VAL", parse(try_from_str = parse_env_var))]
    pub environment: Vec<(String, String)>,
}

async fn run(options: Options) -> Result<()> {
    let addr = options.addr;
    let module_path = PathBuf::from(options.module);

    if !module_path.is_file() {
        bail!("module '{}' does not exist.", module_path.display());
    }

    let module = std::fs::read(&module_path)?;

    let environment = EnvironmentProvider(options.environment);

    let mut server = Server::new(addr, &module, &environment, options.debug_info, true).await?;

    log::info!("Application listening at {}", server);

    let ctrlc = CtrlC::new()?;

    ctrlc
        .race(async move {
            server.accept().await.unwrap();
        })
        .await;

    log::info!("Shutting down...");

    Ok(())
}

#[async_std::main]
async fn main() {
    builder()
        .format_module_path(false)
        .filter_module("wasmtime_functions_runtime", log::LevelFilter::Info)
        .filter_module("wasmtime_functions_host", log::LevelFilter::Info)
        .init();

    if let Err(e) = run(Options::from_args()).await {
        log::error!("{:?}", e);
        std::process::exit(1);
    }
}
