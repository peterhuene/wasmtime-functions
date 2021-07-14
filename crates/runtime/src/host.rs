use anyhow::Result;
use http_types::cookies::SameSite;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::fmt;
use wasmtime::Linker;
use wasmtime_wasi::WasiCtx;

witx_bindgen_wasmtime::import!({
    paths: ["crates/runtime/witx/functions.witx"],
    async: []
});

type Tables = functions::FunctionsTables<Host>;

pub struct Context {
    host: Host,
    request_handle: u32,
    tables: Tables,
    wasi: WasiCtx,
}

impl Context {
    pub fn new(req: crate::server::Request, body: Vec<u8>, wasi: WasiCtx) -> Self {
        let mut tables = Tables::default();
        let request_handle = tables.request_table.insert(Request { inner: req, body });

        Self {
            host: Host {},
            request_handle,
            tables,
            wasi,
        }
    }

    pub fn request_handle(&self) -> u32 {
        self.request_handle
    }

    pub fn take_response(&self, handle: u32) -> Option<tide::Response> {
        self.tables.response_table.get(handle).map(|r| {
            let mut res = r.inner.take().unwrap();
            res.set_body(r.body.take());
            res
        })
    }

    pub fn add_to_linker(linker: &mut Linker<Self>) -> Result<()> {
        wasmtime_wasi::add_to_linker(linker, |s| &mut s.wasi)?;
        functions::add_functions_to_linker(linker, |s| (&mut s.host, &mut s.tables))?;

        Ok(())
    }
}

struct Host;

pub struct Request {
    inner: crate::server::Request,
    body: Vec<u8>,
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Request")
    }
}

#[derive(Debug)]
pub struct Response {
    inner: RefCell<Option<tide::Response>>,
    body: RefCell<Vec<u8>>,
}

// This is temporarily needed as a reference to the resource is captured
// in the future across await points, but is not *used* by multiple threads concurrently.
// TODO: remove this in the future
unsafe impl Sync for Response {}

#[derive(Debug)]
pub struct Cookie {
    inner: RefCell<http_types::Cookie<'static>>,
}

// This is temporarily needed as a reference to the resource is captured
// in the future across await points, but is not *used* by multiple threads concurrently.
// TODO: remove this in the future
unsafe impl Sync for Cookie {}

impl functions::Functions for Host {
    type Cookie = Cookie;
    type Request = Request;
    type Response = Response;

    fn request_method(&mut self, request: &Self::Request) -> String {
        request.inner.method().to_string()
    }

    fn request_uri(&mut self, request: &Self::Request) -> String {
        request.inner.url().as_str().to_string()
    }

    fn request_header(&mut self, request: &Self::Request, name: &str) -> Option<String> {
        request.inner.header(name).map(|v| v.as_str().to_string())
    }

    fn request_cookie(&mut self, request: &Self::Request, name: &str) -> Option<String> {
        request.inner.cookie(name).map(|c| c.value().to_string())
    }

    fn request_param(&mut self, request: &Self::Request, name: &str) -> Option<String> {
        request.inner.param(name).map(ToString::to_string).ok()
    }

    fn request_body(&mut self, request: &Self::Request) -> Vec<u8> {
        request.body.clone()
    }

    fn response_new(&mut self, status: functions::HttpStatus) -> Result<Self::Response, String> {
        Ok(Response {
            inner: RefCell::new(Some(tide::Response::new(
                tide::StatusCode::try_from(status).map_err(|e| e.to_string())?,
            ))),
            body: RefCell::new(Vec::new()),
        })
    }

    fn response_status(&mut self, response: &Self::Response) -> functions::HttpStatus {
        functions::HttpStatus::from(response.inner.borrow().as_ref().unwrap().status())
    }

    fn response_header(&mut self, response: &Self::Response, name: &str) -> Option<String> {
        response
            .inner
            .borrow()
            .as_ref()
            .unwrap()
            .header(name)
            .map(|v| v.as_str().to_string())
    }

    fn response_set_header(&mut self, response: &Self::Response, name: &str, value: &str) {
        response
            .inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .insert_header(name, value);
    }

    fn response_add_cookie(&mut self, response: &Self::Response, cookie: &Self::Cookie) {
        response
            .inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .insert_cookie(cookie.inner.borrow().clone());
    }

    fn response_remove_cookie(&mut self, response: &Self::Response, cookie: &Self::Cookie) {
        response
            .inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .remove_cookie(cookie.inner.borrow().clone());
    }

    fn response_body(&mut self, response: &Self::Response) -> Vec<u8> {
        response.body.borrow().clone()
    }

    fn response_set_body(&mut self, response: &Self::Response, body: &[u8]) {
        let mut b = response.body.borrow_mut();
        b.resize(body.len(), 0);
        b.copy_from_slice(body);
    }

    fn cookie_new(&mut self, name: &str, value: &str) -> Self::Cookie {
        Cookie {
            inner: RefCell::new(http_types::Cookie::new(name.to_string(), value.to_string())),
        }
    }

    fn cookie_set_http_only(&mut self, cookie: &Self::Cookie, enabled: bool) {
        cookie.inner.borrow_mut().set_http_only(Some(enabled))
    }

    fn cookie_set_secure(&mut self, cookie: &Self::Cookie, enabled: bool) {
        cookie.inner.borrow_mut().set_secure(Some(enabled))
    }

    fn cookie_set_max_age(&mut self, cookie: &Self::Cookie, age: i64) {
        cookie
            .inner
            .borrow_mut()
            .set_max_age(Some(time::Duration::seconds(age)))
    }

    fn cookie_set_same_site(&mut self, cookie: &Self::Cookie, policy: functions::SameSitePolicy) {
        cookie.inner.borrow_mut().set_same_site(match policy {
            functions::SameSitePolicy::Strict => SameSite::Strict,
            functions::SameSitePolicy::Lax => SameSite::Lax,
            functions::SameSitePolicy::None => SameSite::None,
        });
    }

    fn cookie_set_domain(&mut self, cookie: &Self::Cookie, domain: &str) {
        cookie.inner.borrow_mut().set_domain(domain.to_string());
    }

    fn cookie_set_path(&mut self, cookie: &Self::Cookie, path: &str) {
        cookie.inner.borrow_mut().set_path(path.to_string());
    }
}
