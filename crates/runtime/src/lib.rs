//! The Wasmtime Functions runtime crate.
//!
//! This crate is responsible for implementing the runtime that hosts Wasmtime Functions applications.

#![deny(missing_docs)]

mod host;
mod server;

pub use server::{EnvironmentProvider, Server};
