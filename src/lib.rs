//! [![GitHub Workflow Status](https://img.shields.io/github/workflow/status/PhotonQuantum/tokio-io-compat/Test?style=flat-square)](https://github.com/PhotonQuantum/tokio-io-compat/actions/workflows/test.yml)
//! [![crates.io](https://img.shields.io/crates/v/tokio-io-compat?style=flat-square)](https://crates.io/crates/tokio-io-compat)
//! [![Documentation](https://img.shields.io/docsrs/tokio-io-compat?style=flat-square)](https://docs.rs/tokio-io-compat)
//!
//! Compatibility wrapper around `std::io::{Read, Write, Seek}` traits that implements `tokio::io::{AsyncRead, AsyncWrite, AsyncSeek}`.
//!
//! Beware that this won't magically make your IO operations asynchronous.
//! You should still consider asyncify your code or move the IO operations to blocking thread if the cost is high.
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

use std::io::{Error, Read, Seek, SeekFrom, Write};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};

#[cfg(test)]
mod tests;

/// The wrapper type.
#[derive(Debug)]
pub struct AsyncIoCompat<T> {
    inner: T,
    last_seek: std::io::Result<u64>,
}

impl<T> AsyncIoCompat<T> {
    /// Create a new wrapper.
    pub fn new(inner: T) -> Self {
        AsyncIoCompat {
            inner,
            last_seek: Ok(0),
        }
    }
    /// Get the inner type.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Read + Unpin> AsyncRead for AsyncIoCompat<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(self.inner.read(buf.initialize_unfilled()).map(|filled| {
            buf.advance(filled);
        }))
    }
}

impl<T: Write + Unpin> AsyncWrite for AsyncIoCompat<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        Poll::Ready(self.inner.write(buf))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(self.inner.flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }
}

impl<T: Seek + Unpin> AsyncSeek for AsyncIoCompat<T> {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> std::io::Result<()> {
        self.last_seek = self.inner.seek(position);
        Ok(())
    }

    fn poll_complete(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<u64>> {
        Poll::Ready(std::mem::replace(&mut self.last_seek, Ok(0)))
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
