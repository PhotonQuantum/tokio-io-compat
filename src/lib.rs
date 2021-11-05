//! [![GitHub Workflow Status](https://img.shields.io/github/workflow/status/PhotonQuantum/tokio-io-compat/Test?style=flat-square)](https://github.com/PhotonQuantum/tokio-io-compat/actions/workflows/test.yml)
//! [![crates.io](https://img.shields.io/crates/v/tokio-io-compat?style=flat-square)](https://crates.io/crates/tokio-io-compat)
//! [![Documentation](https://img.shields.io/docsrs/tokio-io-compat?style=flat-square)](https://docs.rs/tokio-io-compat)
//!
//! Compatibility wrapper around `std::io::{Read, Write, Seek}` traits that implements `tokio::io::{AsyncRead, AsyncWrite, AsyncSeek}`.
//!
//! Beware that this won't magically make your IO operations asynchronous.
//! You should still consider asyncify your code or move the IO operations to blocking thread if the cost is high.
//!
//! ## Deal with `WouldBlock`
//!
//! If you are trying to wrap a non-blocking IO, it may yield [`WouldBlock`](std::io::ErrorKind::WouldBlock) errors when data
//! is not ready.
//! This wrapper will automatically convert [`WouldBlock`](std::io::ErrorKind::WouldBlock) into `Poll::Pending`.
//!
//! However, the waker must be waken later to avoid blocking the future.
//! By default, it is waken immediately. This may waste excessive CPU cycles, especially when the operation
//! is slow.
//!
//! You may add a delay before each wake by creating the wrapper with [`AsyncIoCompat::new_with_delay`](AsyncIoCompat::new_with_delay).
//! If your underlying non-blocking IO has a native poll complete notification mechanism, consider
//! writing your own wrapper instead of using this crate.
//!
//! For reference please see [tokio-tls](https://github.com/tokio-rs/tls/blob/master/tokio-native-tls/src/lib.rs).
//!
//! ## Example
//!
//! ```rust
//! use std::io::Cursor;
//! use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
//! use tokio_io_compat::CompatHelperTrait;
//!
//! # #[tokio::test]
//! # async fn test() {
//! let mut data = Cursor::new(vec![]);
//! data.tokio_io_mut().write_all(&vec![0, 1, 2, 3, 4]).await.unwrap();
//! data.tokio_io_mut().seek(SeekFrom::Start(2)).await.unwrap();
//! assert_eq!(data.tokio_io_mut().read_u8().await.unwrap(), 2);
//! # }
//! ```

use std::io::{Read, Seek, SeekFrom, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use std::{io, mem};

use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

#[cfg(test)]
mod tests;

/// The wrapper type.
#[derive(Debug)]
pub struct AsyncIoCompat<T> {
    inner: T,
    last_seek: io::Result<u64>,
    last_seek_position: SeekFrom,
    wake_delay: Duration,
}

impl<T> AsyncIoCompat<T> {
    /// Create a new wrapper.
    pub const fn new(inner: T) -> Self {
        Self {
            inner,
            last_seek: Ok(0),
            last_seek_position: SeekFrom::Start(0),
            wake_delay: Duration::ZERO,
        }
    }
    /// Create a new wrapper with given [`WouldBlock`](std::io::ErrorKind::WouldBlock) wake delay.
    pub const fn new_with_delay(inner: T, delay: Duration) -> Self {
        Self {
            inner,
            last_seek: Ok(0),
            last_seek_position: SeekFrom::Start(0),
            wake_delay: delay,
        }
    }
    /// Get the inner type.
    #[allow(clippy::missing_const_for_fn)] // false positive
    pub fn into_inner(self) -> T {
        self.inner
    }

    fn schedule_wake(&self, ctx: &Context<'_>) {
        if self.wake_delay.is_zero() {
            ctx.waker().wake_by_ref();
        } else {
            let waker = ctx.waker().clone();
            let delay = self.wake_delay;
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                waker.wake();
            });
        }
    }

    fn no_blocking<F, O>(&mut self, ctx: &Context<'_>, f: F) -> Poll<io::Result<O>>
    where
        F: for<'a> FnOnce(&'a mut Self) -> io::Result<O>,
    {
        match f(self) {
            Ok(t) => Poll::Ready(Ok(t)),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.schedule_wake(ctx);
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl<T: Read + Unpin> AsyncRead for AsyncIoCompat<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.no_blocking(cx, |this| {
            this.inner.read(buf.initialize_unfilled()).map(|filled| {
                buf.advance(filled);
            })
        })
    }
}

impl<T: Write + Unpin> AsyncWrite for AsyncIoCompat<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.no_blocking(cx, |this| this.inner.write(buf))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.no_blocking(cx, |this| this.inner.flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl<T: Seek + Unpin> AsyncSeek for AsyncIoCompat<T> {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        self.last_seek_position = position;
        self.last_seek = self.inner.seek(position);
        Ok(())
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        match self.last_seek {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                let position = self.last_seek_position;
                let res = self.inner.seek(position);
                match res {
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        self.last_seek = res;
                        self.schedule_wake(cx);
                        Poll::Pending
                    }
                    _ => {
                        self.last_seek = Ok(0);
                        Poll::Ready(res)
                    }
                }
            }
            _ => Poll::Ready(mem::replace(&mut self.last_seek, Ok(0))),
        }
    }
}

/// Helper trait that applies [`AsyncIoCompat`](AsyncIoCompat) wrapper to std types.
pub trait CompatHelperTrait {
    /// Applies the [`AsyncIoCompat`](AsyncIoCompat) wrapper by value.
    fn tokio_io(self) -> AsyncIoCompat<Self>
    where
        Self: Sized;
    /// Applies the [`AsyncIoCompat`](AsyncIoCompat) wrapper by mutable reference.
    fn tokio_io_mut(&mut self) -> AsyncIoCompat<&mut Self>;
}

impl<T> CompatHelperTrait for T {
    fn tokio_io(self) -> AsyncIoCompat<Self>
    where
        Self: Sized,
    {
        AsyncIoCompat::new(self)
    }

    fn tokio_io_mut(&mut self) -> AsyncIoCompat<&mut Self> {
        AsyncIoCompat::new(self)
    }
}
