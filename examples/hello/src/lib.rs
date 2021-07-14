use wasmtime_functions::{get, Request};

#[get("/hello/:name")]
fn hello(req: Request) -> String {
    format!("Hello, {}!", req.param("name").unwrap())
}
