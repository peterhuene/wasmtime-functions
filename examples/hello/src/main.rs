use wasmtime_functions::{get, Request};

#[get("/:name")]
fn hello(req: Request) -> String {
    format!("Hello, {}!", req.param("name").unwrap())
}

// See https://github.com/WebAssembly/WASI/issues/24 for tracking issues relating to library wasm modules and WASI
// main is currently required for proper WASI initialization (e.g. environment variables)
// Eventually this will be a cdylib crate without a main
fn main() {}
