//! The Wasmtime Functions codegen crate.
//!
//! This crate is responsible for implementing the procedural macros used in Wasmtime Functions applications.
//!
//! There are two types of macros:
//!
//! * The `http` and verb (e.g. `get`, `post`, `delete`, etc.) macros that define a user's HTTP-triggered function.
//! * The `env` macro that declares a required environment variable.
//!
//! Each macro expands to include a "descriptor" comprising a static array of bytes that is appended to a custom section
//! in the resulting WebAssembly module.
//!
//! Depending on which macros are used, the following custom sections may be present in the WebAssembly module:
//!
//! * The `__functions` section that defines the metadata about user functions and how they can be triggered.
//! * The `__vars` section that defines the metadata about the required environment variables for the application.
//!
//! The `__functions` section is required to run a Wasmtime Functions application, as without it there is nothing for the runtime to do.
//!
//! The `__vars` sections is optional.  It is primarily used by the host to source the required
//! environment variable values when running an application.

#![deny(missing_docs)]

extern crate proc_macro;

use proc_macro::{Span, TokenStream};
use quote::quote;
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    spanned::Spanned,
    Error, FnArg, Ident, ItemFn, LitByteStr, LitStr, Result, Token, Type,
};

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum FunctionTrigger {
    Http { path: String, methods: Vec<Method> },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum FunctionInput {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum FunctionOutput {
    Http,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Function {
    name: String,
    trigger: FunctionTrigger,
    inputs: Vec<FunctionInput>,
    outputs: Vec<FunctionOutput>,
}

fn parse_methods(s: &LitStr) -> Result<Vec<Method>> {
    let mut methods = Vec::new();
    for m in s.value().split(',') {
        methods.push(match m.trim().to_lowercase().as_ref() {
            "get" => Method::Get,
            "head" => Method::Head,
            "post" => Method::Post,
            "put" => Method::Put,
            "delete" => Method::Delete,
            "connect" => Method::Connect,
            "options" => Method::Options,
            "trace" => Method::Trace,
            "path" => Method::Patch,
            _ => {
                return Err(Error::new(
                    s.span(),
                    format!("unsupported HTTP method '{}'", m),
                ))
            }
        });
    }

    if methods.is_empty() {
        return Err(Error::new(s.span(), "at least one HTTP method is required"));
    }

    Ok(methods)
}

fn check_function_validity(func: &ItemFn) -> Result<()> {
    if let Some(constness) = func.sig.constness {
        return Err(Error::new(constness.span, "function cannot be const"));
    }

    if let Some(asyncness) = func.sig.asyncness {
        return Err(Error::new(asyncness.span, "function cannot be async"));
    }

    if let Some(abi) = &func.sig.abi {
        return Err(Error::new(
            abi.extern_token.span,
            "function cannot be extern",
        ));
    }

    if let Some(lt) = func.sig.generics.lt_token {
        return Err(Error::new(lt.spans[0], "function cannot be generic"));
    }

    if let Some(variadic) = &func.sig.variadic {
        return Err(Error::new(
            variadic.dots.spans[0],
            "function cannot be variadic",
        ));
    }

    Ok(())
}

fn check_http_validity(func: &ItemFn) -> Result<()> {
    let inputs = &func.sig.inputs;
    if inputs.is_empty() {
        return Err(Error::new(
            func.sig.ident.span(),
            "function must have a single parameter of type 'Request'",
        ));
    }

    if inputs.len() > 1 {
        return Err(Error::new(
            inputs[1].span(),
            "function cannot have more than one parameter",
        ));
    }

    if let FnArg::Typed(arg) = &inputs[0] {
        if let Type::Path(ty) = &*arg.ty {
            if ty.qself.is_none() {
                if let Some(segment) = ty.path.segments.last() {
                    if segment.ident == "Request" {
                        return Ok(());
                    }
                }
            }
        }
    }

    Err(Error::new(
        inputs[0].span(),
        "parameter must be type 'Request'",
    ))
}

fn emit_descriptor(section: &str, name: &Ident, descriptor: &[u8]) -> proc_macro2::TokenStream {
    // As each descriptor is concatenated in the final Wasm section, prepend with the length
    // so that we can easily iterate each descriptor
    let descriptor_length = descriptor.len() + 4;
    let mut bytes = vec![
        descriptor.len() as u8,
        (descriptor.len() >> 8) as u8,
        (descriptor.len() >> 16) as u8,
        (descriptor.len() >> 24) as u8,
    ];

    bytes.extend_from_slice(descriptor);
    let descriptor_bytes = LitByteStr::new(&bytes, Span::call_site().into());

    quote!(
        #[allow(dead_code)]
        #[link_section = #section]
        #[cfg(target_arch = "wasm32")]
        pub static #name: [u8; #descriptor_length] = *#descriptor_bytes;
    )
}

fn emit_http_function(mut func: ItemFn, path: LitStr, methods: Vec<Method>) -> Result<TokenStream> {
    check_function_validity(&func)?;
    check_http_validity(&func)?;

    let function = Function {
        name: func.sig.ident.to_string(),
        trigger: FunctionTrigger::Http {
            path: path.value(),
            methods,
        },
        inputs: Vec::new(),
        outputs: vec![FunctionOutput::Http],
    };

    let ident = func.sig.ident;
    let inner = Ident::new(&format!("__{}", ident), ident.span());
    let name = Ident::new(
        &format!("__FUNCTION_{}", function.name.to_uppercase()),
        ident.span(),
    );

    func.sig.ident = inner.clone();

    let descriptor = emit_descriptor(
        "__functions",
        &name,
        serde_json::to_string(&[function]).unwrap().as_bytes(),
    );

    Ok(quote!(
        #[no_mangle]
        pub extern "C" fn #ident(req: u32) -> u32 {
            #func

            unsafe {
                wasmtime_functions::Response::from(
                    #inner(wasmtime_functions::Request::from_raw(req))
                )
                .into_raw()
            }
        }

        #descriptor
    )
    .into())
}

/// A macro for declaring an HTTP-triggered function using the `GET` verb.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Get],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `HEAD` verb.
#[proc_macro_attribute]
pub fn head(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Head],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `POST` verb.
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Post],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `PUT` verb.
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Put],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `DELETE` verb.
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Delete],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `CONNECT` verb.
#[proc_macro_attribute]
pub fn connect(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Connect],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `OPTIONS` verb.
#[proc_macro_attribute]
pub fn options(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Options],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `TRACE` verb.
#[proc_macro_attribute]
pub fn trace(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Trace],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function using the `PATCH` verb.
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    match emit_http_function(
        parse_macro_input!(item as ItemFn),
        parse_macro_input!(attr as LitStr),
        vec![Method::Patch],
    ) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring an HTTP-triggered function.
#[proc_macro_attribute]
pub fn http(attr: TokenStream, item: TokenStream) -> TokenStream {
    struct Args {
        methods: LitStr,
        path: LitStr,
    }

    impl Parse for Args {
        fn parse(input: ParseStream) -> Result<Self> {
            let methods = input.parse()?;
            input.parse::<Token![,]>()?;
            let path = input.parse()?;

            Ok(Self { methods, path })
        }
    }

    let args = parse_macro_input!(attr as Args);

    let methods = match parse_methods(&args.methods) {
        Ok(methods) => methods,
        Err(e) => return e.to_compile_error().into(),
    };

    match emit_http_function(parse_macro_input!(item as ItemFn), args.path, methods) {
        Ok(s) => s,
        Err(e) => e.to_compile_error().into(),
    }
}

/// A macro for declaring a required environment variable in a Wasmtime Functions application.
#[proc_macro]
pub fn var(item: TokenStream) -> TokenStream {
    struct Vars {
        vec: Vec<String>,
    }

    impl Parse for Vars {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self {
                vec: input
                    .parse_terminated::<_, Token![,]>(Ident::parse)?
                    .into_iter()
                    .map(|i| i.to_string())
                    .collect(),
            })
        }
    }

    let vars = parse_macro_input!(item as Vars);

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let name = Ident::new(
        &format!("__VAR_{}", COUNTER.fetch_add(1, Ordering::SeqCst)),
        Span::call_site().into(),
    );

    emit_descriptor(
        "__vars",
        &name,
        serde_json::to_string(&vars.vec).unwrap().as_bytes(),
    )
    .into()
}
