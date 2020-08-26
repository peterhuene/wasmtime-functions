# Wasmtime Functions

A demonstration prototype serverless functions runtime built on [Wasmtime](https://github.com/bytecodealliance/wasmtime).

## Getting started

Follow the directions in the [hello example](examples/hello/README.md) to get started.

## What is this?

This is just the runtime for executing *serverless HTTP functions* implemented in [WebAssembly](https://webassembly.org/).

The runtime is capable of instantiating a WebAssembly module and routing HTTP requests to the functions it exposes.

## What is this not?

This **isn't** the *orchestration magic* one might expect from a functions as a service (FaaS) provider, such as on-demand provisioning, horizontal scaling, and load balancing of a serverless application.

In truth, there's really not much "serverless" about what you'll find in this repository.

## Is this even useful?

The runtime itself isn't terribly useful as the functions can only accept HTTP requests, do some computation, and then return a HTTP response.

There is no integration with the various cloud services (e.g. Amazon S3, Azure CosmosDB, etc.) one would expect from a serverless application on popular FaaS offerings, such as Amazon Lambda and Azure Functions.

In fact, the functions have no mechanism yet for doing network requests themselves.  However, a simple HTTP client interface could be added to the runtime in the future.  The HTTP client would then be the basis for the implementation of cloud service SDKs usable from the serverless application.

That said, the runtime *is* a simple demonstration of the potential of using WebAssembly in the serverless space.
