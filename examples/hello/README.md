# Hello example

This is a [simple serverless application](src/lib.rs) that exposes a single HTTP function that responds with a greeting message.

## Install `cargo wasi`

[`cargo wasi`](https://github.com/bytecodealliance/cargo-wasi) is a fantastic tool for easily building Rust crates as WebAssembly modules.

Use `cargo` to install:

```text
$ cargo install cargo-wasi
```

## Running the example

Start with building the example application with `cargo wasi`:

```text
$ cargo wasi build --release
```

This will create a `hello_example.wasm` file in `target/wasm32-wasi/release`.

Next, start the Wasmtime Functions host:

```text
$ cargo run --manifest-path ../../Cargo.toml --release -- target/wasm32-wasi/release/hello_example.wasm --addr 127.0.0.1:3000
[2021-07-15T00:25:40Z INFO ] Adding route for function 'hello' at '/hello/:name' (GET).
[2021-07-15T00:25:40Z INFO ] Application listening at http://127.0.0.1:3000
```

The host will be listening for connections on port 3000.

Lastly, execute the `hello` function:

```text
$ curl localhost:3000/hello/world && echo
Hello, world!
```
