use linkerd2_app_core::{admit, errors, transport::connect};

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

impl<T: connect::ConnectAddr> admit::Admit<T> for PreventLoop {
    type Error = errors::LoopDetected;

    fn admit(&mut self, ep: &T) -> Result<(), Self::Error> {
        let addr = ep.connect_addr();
        tracing::debug!(%addr, self.port);
        if addr.ip().is_loopback() && addr.port() == self.port {
            return Err(errors::LoopDetected::default());
        }
        Ok(())
    }
}
