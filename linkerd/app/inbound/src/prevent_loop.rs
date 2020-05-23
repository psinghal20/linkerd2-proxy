use super::endpoint::Target;
use futures::{future, Poll};
use linkerd2_app_core::{admit, errors, proxy::http, transport::connect};

/// A connection policy that drops
#[derive(Copy, Clone, Debug)]
pub struct PreventLoop {
    port: u16,
}

impl PreventLoop {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

impl tower::Service<Target> for PreventLoop {
    type Response = PreventLoop;
    type Error = errors::LoopDetected;
    type Future = future::FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Err(errors::LoopDetected::default())
    }

    fn call(&mut self, _: Target) -> Self::Future {
        future::err(errors::LoopDetected::default())
    }
}

impl<B> tower::Service<http::Request<B>> for PreventLoop {
    type Response = http::Response<http::boxed::Payload>;
    type Error = errors::LoopDetected;
    type Future = future::FutureResult<Self::Response, Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Err(errors::LoopDetected::default())
    }

    fn call(&mut self, _: http::Request<B>) -> Self::Future {
        future::err(errors::LoopDetected::default())
    }
}

impl<T: connect::ConnectAddr> admit::Admit<T> for PreventLoop {
    type Error = errors::LoopDetected;

    fn admit(&mut self, ep: &T) -> Result<(), Self::Error> {
        let port = ep.connect_addr().port();
        tracing::debug!(port, self.port);
        if port == self.port {
            return Err(errors::LoopDetected::default());
        }

        Ok(())
    }
}
