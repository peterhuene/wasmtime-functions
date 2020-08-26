//! The Wasmtime Functions metadata crate.
//!
//! This crate is responsible for reading the metadata present in a WebAssembly module created by the
//! Wasmtime Functions procedural macros.
//!
//! The data structures defined here should correspond to those in the `wasmtime-functions-codegen` crate.
//!
//! See the documentation of the `wasmtime-functions-codegen` crate for more information.

#![deny(missing_docs)]

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::collections::HashSet;
use wasmparser::{Chunk, Parser, Payload};

/// Represents a HTTP method.
#[derive(Clone, Copy, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    /// The `GET` HTTP method.
    Get,
    /// The `HEAD` HTTP method.
    Head,
    /// The `POST` HTTP method.
    Post,
    /// The `PUT` HTTP method.
    Put,
    /// The `DELETE` HTTP method.
    Delete,
    /// The `CONNECT` HTTP method.
    Connect,
    /// The `OPTIONS` HTTP method.
    Options,
    /// The `TRACE` HTTP method.
    Trace,
    /// The `PATCH` HTTP method.
    Patch,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl AsRef<str> for Method {
    fn as_ref(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Connect => "CONNECT",
            Self::Options => "OPTIONS",
            Self::Trace => "TRACE",
            Self::Patch => "PATCH",
        }
    }
}

impl std::borrow::Borrow<str> for Method {
    fn borrow(&self) -> &str {
        self.as_ref()
    }
}

/// Represents the ways a Wasmtime Function can be triggered.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FunctionTrigger {
    /// The function is triggered by a HTTP request.
    Http {
        /// The request path that triggers the function.
        path: String,
        /// The request methods that trigger the function.
        methods: Vec<Method>,
    },
}

/// Represents an input to a Wasmtime Function.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FunctionInput {}

/// Represents an output of a Wasmtime Function.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum FunctionOutput {
    /// The Wasmtime Function returns a HTTP response.
    Http,
}

/// Represents the metadata of a Wasmtime Function.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Function {
    /// The name of the function.
    pub name: String,
    /// The trigger of the function.
    pub trigger: FunctionTrigger,
    /// The inputs of the function.
    pub inputs: Vec<FunctionInput>,
    /// The outputs of the function.
    pub outputs: Vec<FunctionOutput>,
}

/// Represents the Wasmtime Functions metadata for a WebAssembly module.
pub struct Metadata {
    /// The set of functions exposed in the WebAssembly module.
    pub functions: Vec<Function>,
    /// The set of required environment variables exposed in the WebAssembly module.
    pub vars: Vec<String>,
}

impl Metadata {
    /// Creates a `Metadata` from the bytes of a WebAssembly module.
    pub fn from_module_bytes<T: AsRef<[u8]>>(bytes: &T) -> Result<Self> {
        let mut parser = Parser::new(0);
        let mut offset = 0;
        let bytes = bytes.as_ref();

        let mut functions: Vec<Function> = Vec::new();
        let mut vars: Vec<String> = Vec::new();

        loop {
            if offset >= bytes.len() {
                break;
            }

            match parser.parse(&bytes[offset..], true)? {
                Chunk::NeedMoreData(_) => bail!("the module is not a valid WebAssembly module"),
                Chunk::Parsed { consumed, payload } => {
                    offset += consumed;

                    if let Payload::CustomSection { name, data, .. } = payload {
                        if name == "__functions" {
                            Self::read_section_data(data, &mut functions).map_err(|e| {
                                anyhow!(
                                    "WebAssembly module has an invalid '__functions' section: {}",
                                    e
                                )
                            })?;
                        } else if name == "__vars" {
                            Self::read_section_data(data, &mut vars).map_err(|e| {
                                anyhow!("WebAssembly module has an invalid '__vars' section: {}", e)
                            })?;
                        }
                    }
                }
            }
        }

        let mut set = HashSet::new();
        for f in functions.iter() {
            if !set.insert(&f.name) {
                bail!(
                    "WebAssembly module has a duplicate function named '{}'.",
                    f.name
                );
            }
        }

        set.clear();
        for v in vars.iter() {
            if !set.insert(v) {
                bail!("WebAssembly module has a duplicate variable named '{}'.", v);
            }
        }

        Ok(Self { functions, vars })
    }

    fn read_section_data<'de, T: Deserialize<'de>>(
        data: &'de [u8],
        items: &mut Vec<T>,
    ) -> Result<()> {
        let mut offset = 0;

        loop {
            if offset >= data.len() {
                break;
            }

            match Self::read_data_len(&data[offset..]) {
                Some(len) => {
                    let begin = offset + 4;
                    let end = begin + len;
                    if end > data.len() {
                        bail!("not enough data in the section");
                    }

                    for item in serde_json::from_slice::<Vec<T>>(&data[begin..end])? {
                        items.push(item);
                    }

                    offset = end;
                }
                None => bail!("not enough data in the section"),
            }
        }

        Ok(())
    }

    fn read_data_len(data: &[u8]) -> Option<usize> {
        if data.len() < 4 {
            return None;
        }

        Some(
            (data[0] as usize)
                | ((data[1] as usize) << 8)
                | ((data[2] as usize) << 16)
                | ((data[3] as usize) << 24),
        )
    }
}
