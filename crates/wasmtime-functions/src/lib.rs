//! The Wasmtime Functions crate.
//!
//! This crate defines the API used in Wasmtime Functions applications.

#![deny(missing_docs)]

use http::Uri;
use std::fmt;
use time::Duration;

/// Represents a HTTP status code.
pub type StatusCode = http::StatusCode;

/// Represents a HTTP request.
#[derive(Debug)]
pub struct Request;

impl Request {
    /// Gets the URI of the HTTP request.
    pub fn uri(&self) -> Uri {
        unsafe {
            let len = imports::request::uri_length().unwrap();
            let mut buf = Vec::with_capacity(len);
            imports::request::uri_get(buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            std::str::from_utf8_unchecked(&buf)
                .parse()
                .expect("URI is invalid")
        }
    }

    /// Gets the method of the HTTP request.
    pub fn method(&self) -> String {
        unsafe {
            let len = imports::request::method_length().unwrap();
            if len == 0 {
                return String::new();
            }
            let mut buf = Vec::with_capacity(len);
            imports::request::method_get(buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            String::from_utf8_unchecked(buf)
        }
    }

    /// Gets a header of the HTTP request.
    pub fn header<T: AsRef<str>>(&self, name: T) -> Option<String> {
        unsafe {
            let len = imports::request::header_length(name.as_ref()).unwrap();
            if len == 0 {
                return None;
            }
            let mut buf = Vec::with_capacity(len);
            imports::request::header_get(name.as_ref(), buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            Some(String::from_utf8_unchecked(buf))
        }
    }

    /// Gets a cookie of the HTTP request.
    pub fn cookie<T: AsRef<str>>(&self, name: T) -> Option<String> {
        unsafe {
            let len = imports::request::cookie_length(name.as_ref()).unwrap();
            if len == 0 {
                return None;
            }
            let mut buf = Vec::with_capacity(len);
            imports::request::cookie_get(name.as_ref(), buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            Some(String::from_utf8_unchecked(buf))
        }
    }

    /// Gets a parameter of the HTTP request.
    pub fn param<T: AsRef<str>>(&self, name: T) -> Option<String> {
        unsafe {
            let len = imports::request::param_length(name.as_ref()).unwrap();
            if len == 0 {
                return None;
            }
            let mut buf = Vec::with_capacity(len);
            imports::request::param_get(name.as_ref(), buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            Some(String::from_utf8_unchecked(buf))
        }
    }

    /// Gets the body of the HTTP request.
    pub fn body(&self) -> Vec<u8> {
        unsafe {
            let len = imports::request::body_length().unwrap();
            let mut buf = Vec::with_capacity(len);
            imports::request::body_get(buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            buf
        }
    }
}

/// Used for building HTTP responses.
pub struct ResponseBuilder(Response);

impl ResponseBuilder {
    /// Creates a new HTTP response builder.
    pub fn new(status: StatusCode) -> Self {
        Self(
            unsafe { imports::response::new(status.as_u16()) }
                .map(Response)
                .unwrap(),
        )
    }

    /// Sets a header of the HTTP response.
    pub fn header<T: AsRef<str>, U: AsRef<str>>(self, name: T, value: U) -> Self {
        unsafe { imports::response::header_set((self.0).0, name.as_ref(), value.as_ref()) }
            .unwrap();
        self
    }

    /// Inserts a cookie into the HTTP response.
    pub fn insert_cookie(self, cookie: Cookie) -> Self {
        let handle = cookie.0;
        std::mem::forget(cookie);
        unsafe { imports::response::cookie_insert((self.0).0, handle) }.unwrap();
        self
    }

    /// Removes a cookie in the HTTP response.
    pub fn remove_cookie(self, cookie: Cookie) -> Self {
        let handle = cookie.0;
        std::mem::forget(cookie);
        unsafe { imports::response::cookie_remove((self.0).0, handle) }.unwrap();
        self
    }

    /// Sets the body of the HTTP response.
    ///
    /// This completes the builder and returns the response.
    pub fn body<T: AsRef<[u8]>>(self, body: T) -> Response {
        unsafe { imports::response::body_set((self.0).0, body.as_ref()) }.unwrap();
        self.0
    }
}

/// Represents a HTTP response.
#[derive(Debug)]
pub struct Response(u32);

impl Response {
    /// Creates a new HTTP response builder.
    pub fn build(status: StatusCode) -> ResponseBuilder {
        ResponseBuilder::new(status)
    }

    /// Gets the status code of the HTTP response.
    pub fn status(&self) -> StatusCode {
        unsafe { StatusCode::from_u16(imports::response::status_get(self.0).unwrap()).unwrap() }
    }

    /// Gets a header of the HTTP response.
    pub fn header<T: AsRef<str>>(&self, name: T) -> Option<String> {
        unsafe {
            let len = imports::response::header_length(self.0, name.as_ref()).unwrap();
            if len == 0 {
                return None;
            }
            let mut buf = Vec::with_capacity(len);
            imports::response::header_get(self.0, name.as_ref(), buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            Some(String::from_utf8_unchecked(buf))
        }
    }

    /// Gets the body of the HTTP response.
    pub fn body(&self) -> Vec<u8> {
        unsafe {
            let len = imports::response::body_length(self.0).unwrap();
            let mut buf = Vec::with_capacity(len);
            imports::response::body_get(self.0, buf.as_mut_ptr(), len).unwrap();
            buf.set_len(len);
            buf
        }
    }

    #[doc(hidden)]
    pub unsafe fn from_raw(handle: u32) -> Self {
        Self(handle)
    }

    #[doc(hidden)]
    pub unsafe fn into_raw(self) -> u32 {
        let handle = self.0;
        std::mem::forget(self);
        handle
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe {
            imports::response::free(self.0).unwrap();
        }
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
pub struct CookieBuilder(Cookie);

impl CookieBuilder {
    /// Creates a new HTTP response cookie builder.
    pub fn new(name: &str, value: &str) -> Self {
        unsafe { Self(imports::cookie::new(name, value).map(Cookie).unwrap()) }
    }

    /// Sets the HttpOnly attribute on the cookie.
    pub fn http_only(self) -> Self {
        unsafe { imports::cookie::http_only_set((self.0).0) }.unwrap();
        self
    }

    /// Sets the Secure attribute on the cookie.
    pub fn secure(self) -> Self {
        unsafe { imports::cookie::secure_set((self.0).0) }.unwrap();
        self
    }

    /// Sets the MaxAge attribute on the cookie.
    pub fn max_age(self, value: Duration) -> Self {
        unsafe { imports::cookie::max_age_set((self.0).0, value.whole_seconds()) }.unwrap();
        self
    }

    /// Sets the SameSite attribute on the cookie.
    pub fn same_site(self, value: SameSite) -> Self {
        unsafe {
            imports::cookie::same_site_set(
                (self.0).0,
                match value {
                    SameSite::Strict => imports::types::SAME_SITE_POLICY_STRICT,
                    SameSite::Lax => imports::types::SAME_SITE_POLICY_LAX,
                    SameSite::None => imports::types::SAME_SITE_POLICY_NONE,
                },
            )
        }
        .unwrap();
        self
    }

    /// Sets the Domain attribute on the cookie.
    pub fn domain(self, value: &str) -> Self {
        unsafe { imports::cookie::domain_set((self.0).0, value) }.unwrap();
        self
    }

    /// Sets the Path attribute on the cookie.
    pub fn path(self, value: &str) -> Self {
        unsafe { imports::cookie::path_set((self.0).0, value) }.unwrap();
        self
    }

    /// Finishes building the cookie.
    pub fn finish(self) -> Cookie {
        self.0
    }
}

/// Represents a HTTP response cookie.
pub struct Cookie(u32);

impl Cookie {
    /// Builds a new HTTP response cookie with the given name and value.
    pub fn build<T: AsRef<str>, U: AsRef<str>>(name: T, value: U) -> CookieBuilder {
        CookieBuilder::new(name.as_ref(), value.as_ref())
    }
}

impl Drop for Cookie {
    fn drop(&mut self) {
        unsafe {
            imports::cookie::free(self.0).unwrap();
        }
    }
}

#[allow(clippy::all)]
mod imports {
    wasmtime_functions_codegen::import!(
        "crates/runtime/witx/request.witx",
        "crates/runtime/witx/response.witx",
        "crates/runtime/witx/cookie.witx",
    );
}

pub use wasmtime_functions_codegen::{
    connect, delete, get, head, http, options, patch, post, put, trace, var,
};
