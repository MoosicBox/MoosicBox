use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::SystemTime,
};

pub struct Sleep {
    now: SystemTime,
    ms: u128,
}

impl Sleep {
    #[must_use]
    pub fn new(ms: u128) -> Self {
        Self {
            now: SystemTime::now(),
            ms,
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        if self.now.elapsed().unwrap().as_millis() >= self.ms {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
