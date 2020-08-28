# Hello example

This is a [simple serverless application](src/main.rs) that exposes a single HTTP function that responds with a greeting message.

## Install `cargo wasi`

[`cargo wasi`](https://github.com/bytecodealliance/cargo-wasi) is a fantastic tool for easily building Rust crates as WebAssembly modules.

Use `cargo` to install:

```text
$ cargo install cargo-wasi
```

## Running the example

Start with building the example application with `cargo wasi`:

```text
$ cargo wasi build --release -p hello-example
```

This will create a `hello-example.wasm` file in `target/wasm32-wasi/release`.

Next, start the Wasmtime Functions host:

```
$ cargo run -- target/wasm32-wasi/release/hello-example.wasm --addr 127.0.0.1:3000
```

The host will be listening for connections at port 3000.

Lastly, execute the `hello` function:

```
$ curl -v localhost:3000/world && echo
Hello, world!
```