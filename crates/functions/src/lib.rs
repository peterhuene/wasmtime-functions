//! The Wasmtime Functions crate.
//!
//! This crate defines the API used in Wasmtime Functions applications.

#![deny(missing_docs)]

witx_bindgen_rust::import!("crates/runtime/witx/functions.witx");

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
        functions::request_uri(&self.0)
            .parse()
            .expect("URI is invalid")
    }

    /// Gets the method of the HTTP request.
    pub fn method(&self) -> String {
        functions::request_method(&self.0)
    }

    /// Gets a header of the HTTP request.
    pub fn header<T: AsRef<str>>(&self, name: T) -> Option<String> {
        functions::request_header(&self.0, name.as_ref())
    }

    /// Gets a cookie of the HTTP request.
    pub fn cookie<T: AsRef<str>>(&self, name: T) -> Option<String> {
        functions::request_cookie(&self.0, name.as_ref())
    }

    /// Gets a parameter of the HTTP request.
    pub fn param<T: AsRef<str>>(&self, name: T) -> Option<String> {
        functions::request_param(&self.0, name.as_ref())
    }

    /// Gets the body of the HTTP request.
    pub fn body(&self) -> Vec<u8> {
        functions::request_body(&self.0)
    }
}

/// Used for building HTTP responses.
pub struct ResponseBuilder(functions::Response);

impl ResponseBuilder {
    /// Creates a new HTTP response builder.
    pub fn new(status: StatusCode) -> Self {
        Self(functions::response_new(status.as_u16()).expect("status code is invalid"))
    }

    /// Sets a header of the HTTP response.
    pub fn header<T: AsRef<str>, U: AsRef<str>>(self, name: T, value: U) -> Self {
        functions::response_set_header(&self.0, name.as_ref(), value.as_ref());
        self
    }

    /// Adds a cookie into the HTTP response.
    pub fn add_cookie(self, cookie: &Cookie) -> Self {
        functions::response_add_cookie(&self.0, &cookie.0);
        self
    }

    /// Removes a cookie in the HTTP response.
    pub fn remove_cookie(self, cookie: &Cookie) -> Self {
        functions::response_remove_cookie(&self.0, &cookie.0);
        self
    }

    /// Sets the body of the HTTP response.
    ///
    /// This completes the builder and returns the response.
    pub fn body<T: AsRef<[u8]>>(self, body: T) -> Response {
        functions::response_set_body(&self.0, body.as_ref());
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
        StatusCode::from_u16(functions::response_status(&self.0)).unwrap()
    }

    /// Gets a header of the HTTP response.
    pub fn header<T: AsRef<str>>(&self, name: T) -> Option<String> {
        functions::response_header(&self.0, name.as_ref())
    }

    /// Gets the body of the HTTP response.
    pub fn body(&self) -> Vec<u8> {
        functions::response_body(&self.0)
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
        Self(functions::cookie_new(name.as_ref(), value.as_ref()))
    }

    /// Sets the HttpOnly attribute on the cookie.
    pub fn http_only(self) -> Self {
        functions::cookie_set_http_only(&self.0, true);
        self
    }

    /// Sets the Secure attribute on the cookie.
    pub fn secure(self) -> Self {
        functions::cookie_set_secure(&self.0, true);
        self
    }

    /// Sets the MaxAge attribute on the cookie.
    pub fn max_age(self, value: Duration) -> Self {
        functions::cookie_set_max_age(&self.0, value.whole_seconds());
        self
    }

    /// Sets the SameSite attribute on the cookie.
    pub fn same_site(self, value: SameSite) -> Self {
        functions::cookie_set_same_site(
            &self.0,
            match value {
                SameSite::Strict => functions::SameSitePolicy::Strict,
                SameSite::Lax => functions::SameSitePolicy::Lax,
                SameSite::None => functions::SameSitePolicy::None,
            },
        );
        self
    }

    /// Sets the Domain attribute on the cookie.
    pub fn domain<T: AsRef<str>>(self, value: T) -> Self {
        functions::cookie_set_domain(&self.0, value.as_ref());
        self
    }

    /// Sets the Path attribute on the cookie.
    pub fn path<T: AsRef<str>>(self, value: T) -> Self {
        functions::cookie_set_path(&self.0, value.as_ref());
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
