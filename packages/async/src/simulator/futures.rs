use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use futures::future::FusedFuture;
use pin_project_lite::pin_project;

pin_project! {
    #[derive(Debug, Copy, Clone)]
    pub struct Sleep {
        #[pin]
        now: SystemTime,
        #[pin]
        duration: Duration,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Sleep {
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self {
            now: switchy_time::now(),
            duration,
            polled: false,
            completed: false,
        }
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        log::trace!(
            "Polling Sleep: now={:?} duration={:?} polled={} completed={}",
            this.now,
            this.duration,
            this.polled,
            this.completed,
        );

        let polled = *this.polled;

        if polled {
            let duration = switchy_time::now().duration_since(*this.now).unwrap();
            log::trace!(
                "Sleep polled: {}ms/{}ms",
                duration.as_millis(),
                this.duration.as_millis(),
            );
            if duration >= *this.duration {
                *this.completed.as_mut() = true;
                return Poll::Ready(());
            }
        }

        if !polled {
            *this.polled.as_mut() = true;
        }

        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

impl FusedFuture for Sleep {
    fn is_terminated(&self) -> bool {
        self.completed
    }
}

pin_project! {
    #[derive(Debug, Copy, Clone)]
    pub struct Instant {
        #[pin]
        instant: std::time::Instant,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Instant {
    #[must_use]
    pub const fn new(instant: std::time::Instant) -> Self {
        Self {
            instant,
            polled: false,
            completed: false,
        }
    }
}

fn system_time_to_instant(
    target: SystemTime,
) -> Result<std::time::Instant, std::time::SystemTimeError> {
    let now_sys = SystemTime::now();
    let now_inst = std::time::Instant::now();

    if target >= now_sys {
        // target is in the future (or now)
        let delta: Duration = target.duration_since(now_sys)?;
        Ok(now_inst + delta)
    } else {
        // target is in the past
        let delta: Duration = now_sys.duration_since(target)?;
        Ok(now_inst.checked_sub(delta).unwrap())
    }
}

impl Future for Instant {
    type Output = std::time::Instant;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        log::trace!(
            "Polling Instant: instant={:?} polled={} completed={}",
            this.instant,
            this.polled,
            this.completed,
        );

        let polled = *this.polled;

        if polled {
            let now = system_time_to_instant(switchy_time::now()).unwrap();
            log::trace!("Instant polled: now={:?} instant={:?}", now, this.instant,);
            if now > *this.instant {
                *this.completed.as_mut() = true;
                return Poll::Ready(now);
            }
        }

        if !polled {
            *this.polled.as_mut() = true;
        }

        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

impl FusedFuture for Instant {
    fn is_terminated(&self) -> bool {
        self.completed
    }
}

pin_project! {
    #[derive(Debug, Copy, Clone)]
    pub struct Interval {
        #[pin]
        now: SystemTime,
        #[pin]
        interval: Duration,
        #[pin]
        polled: bool,
        #[pin]
        completed: bool,
    }
}

impl Interval {
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            now: switchy_time::now(),
            interval,
            polled: false,
            completed: false,
        }
    }

    /// # Panics
    ///
    /// * If the `Instant` fails to create
    pub fn tick(&mut self) -> Instant {
        Instant::new(system_time_to_instant(switchy_time::now() + self.interval).unwrap())
    }
}
