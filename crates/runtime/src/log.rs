use tide::{Middleware, Next, Request};

#[derive(Debug, Default, Clone)]
pub struct LogMiddleware;

// A logging middleware similar to the one that comes out-of-the box with
// tide-rs. Unlike tide's, this one doesn't use the structured logging
// experimental feature and thus env-logger works well with it.
struct LogMiddlewareRan;

impl LogMiddleware {
    /// Log a request and a response.
    async fn log<'a, State: Clone + Send + Sync + 'static>(
        &'a self,
        mut req: Request<State>,
        next: Next<'a, State>,
    ) -> tide::Result {
        if req.ext::<LogMiddlewareRan>().is_some() {
            return Ok(next.run(req).await);
        }

        req.set_ext(LogMiddlewareRan);

        let path = req.url().path().to_owned();
        let method = req.method().to_string();

        log::info!("Request received: {} {}", method, path);

        let start = std::time::Instant::now();
        let response = next.run(req).await;
        let elapsed = start.elapsed();

        let status = response.status();

        if status.is_server_error() {
            if let Some(error) = response.error() {
                log::error!(
                    "Internal error: {} {} {} {:?}: {:?}",
                    method,
                    path,
                    status,
                    elapsed,
                    error
                );
            } else {
                log::error!(
                    "Internal error: {} {} {} {:?}",
                    method,
                    path,
                    status,
                    elapsed
                );
            }
        } else if status.is_client_error() {
            if let Some(error) = response.error() {
                log::warn!(
                    "Client error: {} {} {} {:?}: {:?}",
                    method,
                    path,
                    status,
                    elapsed,
                    error
                );
            } else {
                log::warn!("Client error: {} {} {} {:?}", method, path, status, elapsed);
            }
        } else {
            log::info!(
                "Response sent: {} {} {} {:?}",
                method,
                path,
                status,
                elapsed
            );
        }
        Ok(response)
    }
}

#[async_trait::async_trait]
impl<State: Clone + Send + Sync + 'static> Middleware<State> for LogMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> tide::Result {
        self.log(req, next).await
    }
}
