use std::{
    pin::{pin, Pin},
    task::{Context, Poll},
};

use futures_channel::mpsc::{TrySendError, UnboundedReceiver, UnboundedSender};
use futures_core::{FusedStream, Stream};

pub struct MoosicBoxUnboundedSender<T>(UnboundedSender<T>);

impl<T> Clone for MoosicBoxUnboundedSender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> MoosicBoxUnboundedSender<T> {
    pub fn unbounded_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.0.unbounded_send(msg)
    }
}

pub struct MoosicBoxUnboundedReceiver<T>(UnboundedReceiver<T>);

impl<T> FusedStream for MoosicBoxUnboundedReceiver<T> {
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

impl<T> Stream for MoosicBoxUnboundedReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let stream = pin!(self);
        stream.poll_next(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

pub fn unbounded<T>() -> (MoosicBoxUnboundedSender<T>, MoosicBoxUnboundedReceiver<T>) {
    let (tx, rx) = futures_channel::mpsc::unbounded();

    (MoosicBoxUnboundedSender(tx), MoosicBoxUnboundedReceiver(rx))
}
