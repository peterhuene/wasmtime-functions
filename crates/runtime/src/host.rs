use crate::server::Request;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::rc::Rc;
use tide::StatusCode;
use wiggle::{GuestError, GuestErrorType, GuestPtr};

wiggle::from_witx!({
    witx: [
        "crates/runtime/witx/functions.witx",
    ],
});

// In the future, we should use an externref for handles
// Until the reference types proposal is fully supported in the toolchain, use a map from integer "handle" to content
enum HandleContent {
    Response(tide::Response, Vec<u8>),
    Cookie(http_types::Cookie<'static>),
}

struct HostContextInner {
    request: (Request, Vec<u8>),
    handles: HashMap<u32, HandleContent>,
    last_handle: u32,
}

impl HostContextInner {
    fn insert_response(&mut self, status: u16) -> Result<types::ResponseHandle, types::Error> {
        let response = tide::Response::new(
            StatusCode::try_from(status).map_err(|_| types::Error::InvalidArgument)?,
        );

        let handle = self.next_handle_value()?;

        self.handles
            .insert(handle, HandleContent::Response(response, Vec::new()));

        Ok(handle.into())
    }

    fn insert_cookie(
        &mut self,
        name: &str,
        value: &str,
    ) -> Result<types::CookieHandle, types::Error> {
        let cookie = http_types::Cookie::new(name.to_string(), value.to_string());

        let handle = self.next_handle_value()?;

        self.handles.insert(handle, HandleContent::Cookie(cookie));

        Ok(handle.into())
    }

    fn remove_response(
        &mut self,
        handle: types::ResponseHandle,
    ) -> Result<(tide::Response, Vec<u8>), types::Error> {
        if self.get_response(handle).is_none() {
            return Err(types::Error::InvalidHandle);
        }

        match self.handles.remove(&handle.into()) {
            Some(HandleContent::Response(r, b)) => Ok((r, b)),
            _ => unreachable!(),
        }
    }

    fn remove_cookie(
        &mut self,
        handle: types::CookieHandle,
    ) -> Result<http_types::Cookie<'static>, types::Error> {
        if self.get_cookie_mut(handle).is_none() {
            return Err(types::Error::InvalidHandle);
        }

        match self.handles.remove(&handle.into()) {
            Some(HandleContent::Cookie(c)) => Ok(c),
            _ => unreachable!(),
        }
    }

    fn get_response(&self, handle: types::ResponseHandle) -> Option<(&tide::Response, &Vec<u8>)> {
        match self.handles.get(&handle.into()) {
            Some(HandleContent::Response(r, b)) => Some((r, b)),
            _ => None,
        }
    }

    fn get_response_mut(
        &mut self,
        handle: types::ResponseHandle,
    ) -> Option<(&mut tide::Response, &mut Vec<u8>)> {
        match self.handles.get_mut(&handle.into()) {
            Some(HandleContent::Response(r, b)) => Some((r, b)),
            _ => None,
        }
    }

    fn get_cookie_mut(
        &mut self,
        handle: types::CookieHandle,
    ) -> Option<&mut http_types::Cookie<'static>> {
        match self.handles.get_mut(&handle.into()) {
            Some(HandleContent::Cookie(c)) => Some(c),
            _ => None,
        }
    }

    fn next_handle_value(&mut self) -> Result<u32, types::Error> {
        match self.last_handle.checked_add(1) {
            Some(n) => self.last_handle = n,
            None => return Err(types::Error::HandlesExhausted),
        };

        Ok(self.last_handle)
    }
}

#[derive(Clone)]
pub struct HostContext(Rc<RefCell<HostContextInner>>);

impl HostContext {
    pub fn new(request: Request, body: Vec<u8>) -> Self {
        Self(Rc::new(RefCell::new(HostContextInner {
            request: (request, body),
            handles: HashMap::new(),
            last_handle: 0,
        })))
    }

    pub fn take_response(&self, handle: types::ResponseHandle) -> Option<tide::Response> {
        self.inner_mut()
            .remove_response(handle)
            .ok()
            .map(|(mut r, b)| {
                r.set_body(b);
                r
            })
    }

    fn inner(&self) -> Ref<HostContextInner> {
        self.0.borrow()
    }

    fn inner_mut(&self) -> RefMut<HostContextInner> {
        self.0.borrow_mut()
    }
}

impl functions::Functions for HostContext {
    fn request_method_length(&self) -> Result<u32, types::Error> {
        Ok(self.inner().request.0.method().as_ref().len() as u32)
    }

    fn request_method_get(
        &self,
        buffer: &GuestPtr<u8>,
        buffer_len: u32,
    ) -> Result<(), types::Error> {
        buffer
            .as_array(buffer_len)
            .copy_from_slice(self.inner().request.0.method().as_ref().as_bytes())?;

        Ok(())
    }

    fn request_uri_length(&self) -> Result<u32, types::Error> {
        Ok(self.inner().request.0.url().as_str().len() as u32)
    }

    fn request_uri_get(&self, buffer: &GuestPtr<u8>, buffer_len: u32) -> Result<(), types::Error> {
        buffer
            .as_array(buffer_len)
            .copy_from_slice(self.inner().request.0.url().as_str().as_bytes())?;

        Ok(())
    }

    fn request_header_length(&self, name: &GuestPtr<str>) -> Result<u32, types::Error> {
        Ok(match self.inner().request.0.header(&*name.as_str()?) {
            Some(v) => v.as_str().len() as u32,
            None => 0,
        })
    }

    fn request_header_get(
        &self,
        name: &GuestPtr<str>,
        buffer: &GuestPtr<u8>,
        buffer_len: u32,
    ) -> Result<(), types::Error> {
        if let Some(v) = self.inner().request.0.header(&*name.as_str()?) {
            buffer
                .as_array(buffer_len)
                .copy_from_slice(v.as_str().as_bytes())?;
        }

        Ok(())
    }

    fn request_cookie_length(&self, name: &GuestPtr<str>) -> Result<u32, types::Error> {
        Ok(match self.inner().request.0.cookie(&*name.as_str()?) {
            Some(c) => c.value().len() as u32,
            None => 0,
        })
    }

    fn request_cookie_get(
        &self,
        name: &GuestPtr<str>,
        buffer: &GuestPtr<u8>,
        buffer_len: u32,
    ) -> Result<(), types::Error> {
        if let Some(c) = self.inner().request.0.cookie(&*name.as_str()?) {
            buffer
                .as_array(buffer_len)
                .copy_from_slice(c.value().as_bytes())?;
        }

        Ok(())
    }

    fn request_param_length(&self, name: &GuestPtr<str>) -> Result<u32, types::Error> {
        Ok(match self.inner().request.0.param(&*name.as_str()?).ok() {
            Some(p) => p.len() as u32,
            None => 0,
        })
    }

    fn request_param_get(
        &self,
        name: &GuestPtr<str>,
        buffer: &GuestPtr<u8>,
        buffer_len: u32,
    ) -> Result<(), types::Error> {
        if let Ok(p) = self.inner().request.0.param(&*name.as_str()?) {
            buffer.as_array(buffer_len).copy_from_slice(p.as_bytes())?;
        }

        Ok(())
    }

    fn request_body_length(&self) -> Result<u32, types::Error> {
        Ok(self.inner().request.1.len() as u32)
    }

    fn request_body_get(&self, buffer: &GuestPtr<u8>, buffer_len: u32) -> Result<(), types::Error> {
        buffer
            .as_array(buffer_len)
            .copy_from_slice(&self.inner().request.1)?;

        Ok(())
    }

    fn response_new(&self, status: u16) -> Result<types::ResponseHandle, types::Error> {
        self.inner_mut().insert_response(status)
    }

    fn response_free(&self, response: types::ResponseHandle) -> Result<(), types::Error> {
        self.inner_mut().remove_response(response).map(|_| ())
    }

    fn response_status_get(&self, response: types::ResponseHandle) -> Result<u16, types::Error> {
        Ok(u16::from(
            self.inner()
                .get_response(response)
                .ok_or(types::Error::InvalidHandle)?
                .0
                .status(),
        ))
    }

    fn response_header_length(
        &self,
        response: types::ResponseHandle,
        name: &GuestPtr<str>,
    ) -> Result<u32, types::Error> {
        Ok(
            match self
                .inner()
                .get_response(response)
                .ok_or(types::Error::InvalidHandle)?
                .0
                .header(&*name.as_str()?)
            {
                Some(v) => v.as_str().len() as u32,
                None => 0,
            },
        )
    }

    fn response_header_get(
        &self,
        response: types::ResponseHandle,
        name: &GuestPtr<str>,
        buffer: &GuestPtr<u8>,
        buffer_len: u32,
    ) -> Result<(), types::Error> {
        if let Some(v) = self
            .inner()
            .get_response(response)
            .ok_or(types::Error::InvalidHandle)?
            .0
            .header(&*name.as_str()?)
        {
            buffer
                .as_array(buffer_len)
                .copy_from_slice(v.as_str().as_bytes())?;
        }

        Ok(())
    }

    fn response_header_set(
        &self,
        response: types::ResponseHandle,
        name: &GuestPtr<str>,
        value: &GuestPtr<str>,
    ) -> Result<(), types::Error> {
        self.inner_mut()
            .get_response_mut(response)
            .ok_or(types::Error::InvalidHandle)?
            .0
            .insert_header(&*name.as_str()?, &*value.as_str()?);

        Ok(())
    }

    fn response_cookie_insert(
        &self,
        response: types::ResponseHandle,
        cookie: types::CookieHandle,
    ) -> Result<(), types::Error> {
        let cookie = self.inner_mut().remove_cookie(cookie)?;

        self.inner_mut()
            .get_response_mut(response)
            .ok_or(types::Error::InvalidHandle)?
            .0
            .insert_cookie(cookie);

        Ok(())
    }

    fn response_cookie_remove(
        &self,
        response: types::ResponseHandle,
        cookie: types::CookieHandle,
    ) -> Result<(), types::Error> {
        let cookie = self.inner_mut().remove_cookie(cookie)?;

        self.inner_mut()
            .get_response_mut(response)
            .ok_or(types::Error::InvalidHandle)?
            .0
            .remove_cookie(cookie);

        Ok(())
    }

    fn response_body_length(&self, response: types::ResponseHandle) -> Result<u32, types::Error> {
        Ok(self
            .inner()
            .get_response(response)
            .ok_or(types::Error::InvalidHandle)?
            .1
            .len() as u32)
    }

    fn response_body_get(
        &self,
        response: types::ResponseHandle,
        buffer: &GuestPtr<u8>,
        buffer_len: u32,
    ) -> Result<(), types::Error> {
        buffer.as_array(buffer_len).copy_from_slice(
            &self
                .inner()
                .get_response(response)
                .ok_or(types::Error::InvalidHandle)?
                .1,
        )?;

        Ok(())
    }

    fn response_body_set(
        &self,
        response: types::ResponseHandle,
        bytes: &GuestPtr<[u8]>,
    ) -> Result<(), types::Error> {
        let mut inner = self.inner_mut();

        let body = inner
            .get_response_mut(response)
            .ok_or(types::Error::InvalidHandle)?
            .1;

        body.resize(bytes.len() as usize, 0);
        body.copy_from_slice(&bytes.as_slice()?);

        Ok(())
    }

    fn cookie_new(
        &self,
        name: &GuestPtr<str>,
        value: &GuestPtr<str>,
    ) -> Result<types::CookieHandle, types::Error> {
        self.inner_mut()
            .insert_cookie(&name.as_str()?, &value.as_str()?)
    }

    fn cookie_free(&self, cookie: types::CookieHandle) -> Result<(), types::Error> {
        self.inner_mut().remove_cookie(cookie).map(|_| ())
    }

    fn cookie_http_only_set(&self, cookie: types::CookieHandle) -> Result<(), types::Error> {
        self.inner_mut()
            .get_cookie_mut(cookie)
            .ok_or(types::Error::InvalidHandle)?
            .set_http_only(true);

        Ok(())
    }

    fn cookie_secure_set(&self, cookie: types::CookieHandle) -> Result<(), types::Error> {
        self.inner_mut()
            .get_cookie_mut(cookie)
            .ok_or(types::Error::InvalidHandle)?
            .set_secure(true);

        Ok(())
    }

    fn cookie_max_age_set(
        &self,
        cookie: types::CookieHandle,
        max_age: i64,
    ) -> Result<(), types::Error> {
        self.inner_mut()
            .get_cookie_mut(cookie)
            .ok_or(types::Error::InvalidHandle)?
            .set_max_age(Some(time::Duration::seconds(max_age)));

        Ok(())
    }

    fn cookie_same_site_set(
        &self,
        cookie: types::CookieHandle,
        same_site: types::SameSitePolicy,
    ) -> Result<(), types::Error> {
        use http_types::cookies::SameSite;

        self.inner_mut()
            .get_cookie_mut(cookie)
            .ok_or(types::Error::InvalidHandle)?
            .set_same_site(match same_site {
                types::SameSitePolicy::Strict => SameSite::Strict,
                types::SameSitePolicy::Lax => SameSite::Lax,
                types::SameSitePolicy::None => SameSite::None,
            });

        Ok(())
    }

    fn cookie_domain_set(
        &self,
        cookie: types::CookieHandle,
        domain: &GuestPtr<str>,
    ) -> Result<(), types::Error> {
        self.inner_mut()
            .get_cookie_mut(cookie)
            .ok_or(types::Error::InvalidHandle)?
            .set_domain(domain.as_str()?.to_string());

        Ok(())
    }

    fn cookie_path_set(
        &self,
        cookie: types::CookieHandle,
        path: &GuestPtr<str>,
    ) -> Result<(), types::Error> {
        self.inner_mut()
            .get_cookie_mut(cookie)
            .ok_or(types::Error::InvalidHandle)?
            .set_path(path.as_str()?.to_string());

        Ok(())
    }
}

impl GuestErrorType for types::Error {
    fn success() -> Self {
        Self::Ok
    }
}

impl From<GuestError> for types::Error {
    fn from(err: GuestError) -> Self {
        match err {
            GuestError::InvalidFlagValue { .. } => Self::InvalidArgument,
            GuestError::InvalidEnumValue { .. } => Self::InvalidArgument,
            GuestError::PtrOverflow { .. } => Self::InvalidPointer,
            GuestError::PtrOutOfBounds { .. } => Self::InvalidPointer,
            GuestError::PtrNotAligned { .. } => Self::InvalidArgument,
            GuestError::PtrBorrowed { .. } => Self::InvalidPointer,
            GuestError::InvalidUtf8 { .. } => Self::InvalidUtf8,
            GuestError::TryFromIntError { .. } => Self::IntegerOverflow,
            GuestError::InFunc { err, .. } => (*err).into(),
            GuestError::SliceLengthsDiffer { .. } => Self::InvalidLength,
            GuestError::BorrowCheckerOutOfHandles { .. } => Self::InvalidPointer,
        }
    }
}

wasmtime_wiggle::wasmtime_integration!({
    target: self,
    witx: [
        "crates/runtime/witx/functions.witx",
    ],
    ctx: HostContext,
    modules: {
        functions => {
            name: Functions,
            docs: "Represents the linkable host functions for the `functions` module.",
        },
    },
});
