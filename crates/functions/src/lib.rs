//! The Wasmtime Functions crate.
//!
//! This crate defines the API used in Wasmtime Functions applications.

#![deny(missing_docs)]

witx_bindgen_rust::import!("../../crates/runtime/witx/functions.witx");

use http::Uri;
use std::fmt;
use time::Duration;

/// Represents a HTTP status code.
pub type StatusCode = http::StatusCode;

/// Represents a HTTP request.
#[derive(Debug)]
pub struct Request(functions::Request);

impl Request {
    #[doc(hidden)]
    pub unsafe fn from_raw(handle: u32) -> Self {
        Self(functions::Request::from_raw(handle as i32))
    }

    /// Gets the URI of the HTTP request.
    pub fn uri(&self) -> Uri {
        self.0.uri().parse().expect("URI is invalid")
    }

    /// Gets the method of the HTTP request.
    pub fn method(&self) -> String {
        self.0.method()
    }

    /// Gets a header of the HTTP request.
    pub fn header<T: AsRef<str>>(&self, name: T) -> Option<String> {
        self.0.header(name.as_ref())
    }

    /// Gets a cookie of the HTTP request.
    pub fn cookie<T: AsRef<str>>(&self, name: T) -> Option<String> {
        self.0.cookie(name.as_ref())
    }

    /// Gets a parameter of the HTTP request.
    pub fn param<T: AsRef<str>>(&self, name: T) -> Option<String> {
        self.0.param(name.as_ref())
    }

    /// Gets the body of the HTTP request.
    pub fn body(&self) -> Result<Vec<u8>, String> {
        self.0.body()
    }
}

/// Used for building HTTP responses.
pub struct ResponseBuilder(functions::Response);

impl ResponseBuilder {
    /// Creates a new HTTP response builder.
    pub fn new(status: StatusCode) -> Self {
        Self(functions::Response::new(status.as_u16()).expect("status code is invalid"))
    }

    /// Sets a header of the HTTP response.
    pub fn header<T: AsRef<str>, U: AsRef<str>>(self, name: T, value: U) -> Self {
        self.0.set_header(name.as_ref(), value.as_ref());
        self
    }

    /// Adds a cookie into the HTTP response.
    pub fn add_cookie(self, cookie: &Cookie) -> Self {
        self.0.add_cookie(&cookie.0);
        self
    }

    /// Removes a cookie in the HTTP response.
    pub fn remove_cookie(self, cookie: &Cookie) -> Self {
        self.0.remove_cookie(&cookie.0);
        self
    }

    /// Sets the body of the HTTP response.
    ///
    /// This completes the builder and returns the response.
    pub fn body<T: AsRef<[u8]>>(self, body: T) -> Response {
        self.0.set_body(body.as_ref());
        Response(self.0)
    }
}

/// Represents a HTTP response.
#[derive(Debug)]
pub struct Response(functions::Response);

impl Response {
    /// Creates a new HTTP response builder.
    pub fn build(status: StatusCode) -> ResponseBuilder {
        ResponseBuilder::new(status)
    }

    /// Gets the status code of the HTTP response.
    pub fn status(&self) -> StatusCode {
        StatusCode::from_u16(self.0.status()).unwrap()
    }

    /// Gets a header of the HTTP response.
    pub fn header<T: AsRef<str>>(&self, name: T) -> Option<String> {
        self.0.header(name.as_ref())
    }

    /// Gets the body of the HTTP response.
    pub fn body(&self) -> Vec<u8> {
        self.0.body()
    }

    #[doc(hidden)]
    pub unsafe fn into_raw(self) -> u32 {
        self.0.into_raw() as u32
    }
}

impl From<()> for Response {
    fn from(_: ()) -> Self {
        Self::build(StatusCode::NO_CONTENT).body("")
    }
}

impl<E: fmt::Display> From<std::result::Result<Response, E>> for Response {
    fn from(res: std::result::Result<Response, E>) -> Self {
        res.unwrap_or_else(|e| {
            Self::build(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(e.to_string())
        })
    }
}

impl From<String> for Response {
    fn from(s: String) -> Self {
        Self::build(StatusCode::OK)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(s)
    }
}

/// The `SameSite` cookie attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SameSite {
    /// Cookies will only be sent in a first-party context and not be sent along with requests initiated by third party websites.
    Strict,
    /// Cookies are allowed to be sent with top-level navigations and will be sent along with GET request initiated by third party website.
    Lax,
    /// Cookies will be sent in all contexts, i.e sending cross-origin is allowed.
    None,
}

/// Used for building HTTP response cookies.
pub struct CookieBuilder(functions::Cookie);

impl CookieBuilder {
    /// Creates a new HTTP response cookie builder.
    pub fn new<T: AsRef<str>, U: AsRef<str>>(name: T, value: U) -> Self {
        Self(functions::Cookie::new(name.as_ref(), value.as_ref()))
    }

    /// Sets the HttpOnly attribute on the cookie.
    pub fn http_only(self) -> Self {
        self.0.set_http_only(true);
        self
    }

    /// Sets the Secure attribute on the cookie.
    pub fn secure(self) -> Self {
        self.0.set_secure(true);
        self
    }

    /// Sets the MaxAge attribute on the cookie.
    pub fn max_age(self, value: Duration) -> Self {
        self.0.set_max_age(value.whole_seconds());
        self
    }

    /// Sets the SameSite attribute on the cookie.
    pub fn same_site(self, value: SameSite) -> Self {
        self.0.set_same_site(match value {
            SameSite::Strict => functions::SameSitePolicy::Strict,
            SameSite::Lax => functions::SameSitePolicy::Lax,
            SameSite::None => functions::SameSitePolicy::None,
        });
        self
    }

    /// Sets the Domain attribute on the cookie.
    pub fn domain<T: AsRef<str>>(self, value: T) -> Self {
        self.0.set_domain(value.as_ref());
        self
    }

    /// Sets the Path attribute on the cookie.
    pub fn path<T: AsRef<str>>(self, value: T) -> Self {
        self.0.set_path(value.as_ref());
        self
    }

    /// Finishes building the cookie.
    pub fn finish(self) -> Cookie {
        Cookie(self.0)
    }
}

/// Represents a HTTP response cookie.
pub struct Cookie(functions::Cookie);

impl Cookie {
    /// Builds a new HTTP response cookie with the given name and value.
    pub fn build<T: AsRef<str>, U: AsRef<str>>(name: T, value: U) -> CookieBuilder {
        CookieBuilder::new(name.as_ref(), value.as_ref())
    }
}

pub use wasmtime_functions_codegen::{
    connect, delete, get, head, http, options, patch, post, put, trace, var,
};
