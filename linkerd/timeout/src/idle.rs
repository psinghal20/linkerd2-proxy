use crate::error::HumanDuration;
use futures::{future, Future, Poll};
use linkerd2_error::Error;
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use std::time::{Duration, Instant};
use tokio_timer::Delay;

#[derive(Clone, Debug)]
pub struct IdleLayer {
    timeout: Duration,
    jitter: Duration,
    rng: SmallRng,
}

#[derive(Debug)]
pub struct Idle<S> {
    inner: S,
    idle: Delay,
    current: Duration,
    config: IdleLayer,
}

#[derive(Copy, Clone, Debug)]
pub struct IdleError(Duration);

// === impl IdleLayer ===

impl IdleLayer {
    pub fn new(timeout: Duration, jitter: Duration) -> Self {
        let rng = SmallRng::from_entropy();
        Self {
            timeout,
            jitter,
            rng,
        }
    }

    fn next_timeout(&mut self) -> Duration {
        Duration::from_millis(self.rng.next_u64() % self.jitter.as_millis() as u64)
    }
}

impl<S> tower::layer::Layer<S> for IdleLayer {
    type Service = Idle<S>;

    fn layer(&self, inner: S) -> Self::Service {
        let mut config = self.clone();
        let current = config.next_timeout();
        Self::Service {
            inner,
            idle: Delay::new(Instant::now() + current),
            current,
            config,
        }
    }
}

// === impl Idle ===

impl<S, T> tower::Service<T> for Idle<S>
where
    S: tower::Service<T>,
    S::Error: Into<Error>,
{
    type Response = S::Response;
    type Error = Error;
    type Future = future::MapErr<S::Future, fn(S::Error) -> Self::Error>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if self.idle.poll().expect("timer must succeed").is_ready() {
            return Err(IdleError(self.current).into());
        }

        self.inner.poll_ready().map_err(Into::into)
    }

    fn call(&mut self, req: T) -> Self::Future {
        self.idle.reset(Instant::now() + self.config.next_timeout());
        self.inner.call(req).map_err(Into::into)
    }
}

// === impl IdleError ===

impl std::fmt::Display for IdleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Service idled out after {}", HumanDuration(&self.0))
    }
}

impl std::error::Error for IdleError {}

#[cfg(test)]
mod test {
    use super::IdleLayer;
    use futures::{future, Future};
    use std::time::{Duration, Instant};
    use tower::layer::Layer;
    use tower::Service;
    use tower_test::mock;

    fn run<F, R>(f: F)
    where
        F: FnOnce() -> R + 'static,
        R: future::IntoFuture<Item = ()> + 'static,
    {
        tokio::runtime::current_thread::run(future::lazy(f).map_err(|_| panic!("Failed")));
    }

    #[test]
    fn call_succeeds_when_idle() {
        let timeout = Duration::from_millis(100);
        let (service, mut handle) = mock::pair::<(), ()>();
        let mut service = IdleLayer::new(timeout).layer(service);

        run(move || {
            // The inner starts available.
            handle.allow(1);
            assert!(service.poll_ready().unwrap().is_ready());

            // Then we wait for the idle timeout, at which point the service
            // should still be usable if we don't poll_ready again.
            tokio_timer::Delay::new(Instant::now() + timeout + Duration::from_millis(1))
                .map_err(|_| ())
                .and_then(move |_| {
                    let fut = service.call(());
                    let ((), rsp) = handle.next_request().expect("must get a request");
                    rsp.send_response(());
                    fut.map_err(|_| ()).map(move |()| {
                        // Service remains usable.
                        assert!(service.poll_ready().unwrap().is_not_ready());
                        let _ = handle;
                    })
                })
        });
    }

    #[test]
    fn poll_ready_fails_after_idle() {
        let timeout = Duration::from_millis(100);
        let (service, mut handle) = mock::pair::<(), ()>();
        let mut service = IdleLayer::new(timeout).layer(service);

        run(move || {
            // The inner starts available.
            handle.allow(1);
            assert!(service.poll_ready().unwrap().is_ready());

            // Then we wait for the idle timeout, at which point the service
            // should fail.
            tokio_timer::Delay::new(Instant::now() + timeout + Duration::from_millis(1))
                .map_err(|_| ())
                .map(move |_| {
                    assert!(service
                        .poll_ready()
                        .expect_err("must fail")
                        .is::<super::IdleError>());
                })
        });
    }
}
