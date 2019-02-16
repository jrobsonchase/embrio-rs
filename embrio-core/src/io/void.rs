use core::{
    pin::Pin,
    task::{self, Poll},
};

use crate::io::Write;

pub struct Void {
    _marker: (),
}

pub fn void() -> Void {
    Void { _marker: () }
}

impl Write for Void {
    type Error = !;

    fn poll_write(
        self: Pin<&mut Self>,
        _waker: &task::Waker,
        buf: &[u8],
    ) -> Poll<Result<usize, Self::Error>> {
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _waker: &task::Waker,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        self: Pin<&mut Self>,
        _waker: &task::Waker,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
