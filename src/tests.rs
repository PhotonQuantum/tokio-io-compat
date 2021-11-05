use std::io;
use std::io::{Cursor, ErrorKind, Read, Seek, Write};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use crate::AsyncIoCompat;

struct ChunkedRead<T>(T, usize);

impl<T: Read> Read for ChunkedRead<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        assert!(buf.len() > self.1);
        let mut chunk = vec![0; self.1];
        let size = self.0.read(&mut chunk).unwrap();
        buf[..self.1].copy_from_slice(&*chunk);
        Ok(size)
    }
}

struct FailSeek;

impl Seek for FailSeek {
    fn seek(&mut self, _pos: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(ErrorKind::Other, "boom"))
    }
}

struct NonBlockingIO<T> {
    buf: T,
    read: usize,
    write: usize,
    flush: usize,
    seek: usize,
}

impl<T> NonBlockingIO<T> {
    fn may_block<F, FC, O>(&mut self, count: FC, f: F) -> io::Result<O>
    where
        F: for<'a> FnOnce(&'a mut T) -> io::Result<O>,
        FC: for<'a> FnOnce(&'a mut Self) -> &'a mut usize,
    {
        let i = count(self);
        if *i == 0 {
            f(&mut self.buf)
        } else {
            *i -= 1;
            Err(io::Error::new(io::ErrorKind::WouldBlock, ""))
        }
    }
}

impl<T: Read> Read for NonBlockingIO<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.may_block(|this| &mut this.read, |inner| inner.read(buf))
    }
}

impl<T: Write> Write for NonBlockingIO<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.may_block(|this| &mut this.write, |inner| inner.write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.may_block(|this| &mut this.flush, |inner| inner.flush())
    }
}

impl<T: Seek> Seek for NonBlockingIO<T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.may_block(|this| &mut this.seek, |inner| inner.seek(pos))
    }
}

#[tokio::test]
async fn must_read_write() {
    let data: Vec<u8> = (0..127).into_iter().cycle().take(10000).collect();
    let mut src = AsyncIoCompat::new(Cursor::new(data.clone()));
    let mut dest = AsyncIoCompat::new(Cursor::new(vec![]));

    tokio::io::copy(&mut src, &mut dest).await.unwrap();

    assert_eq!(
        src.into_inner().into_inner(),
        dest.into_inner().into_inner()
    );
}

#[tokio::test]
async fn must_read_write_partial_buf() {
    let data: Vec<u8> = (0..127).into_iter().cycle().take(10000).collect();
    let mut src = AsyncIoCompat::new(ChunkedRead(Cursor::new(data.clone()), 1000));
    let mut dest = AsyncIoCompat::new(Cursor::new(vec![]));

    tokio::io::copy(&mut src, &mut dest).await.unwrap();

    assert_eq!(
        src.into_inner().0.into_inner(),
        dest.into_inner().into_inner()
    );
}

#[tokio::test]
async fn must_seek() {
    let data: Vec<u8> = (0..127).collect();
    let mut buf = AsyncIoCompat::new(Cursor::new(data));
    assert_eq!(buf.seek(SeekFrom::Start(10)).await.unwrap(), 10);
    assert_eq!(buf.read_u8().await.unwrap(), 10);
    assert_eq!(buf.seek(SeekFrom::Current(20)).await.unwrap(), 31);
    assert_eq!(buf.read_u8().await.unwrap(), 31);
    assert_eq!(buf.seek(SeekFrom::Current(-5)).await.unwrap(), 27);
    assert_eq!(buf.read_u8().await.unwrap(), 27);
    assert_eq!(buf.seek(SeekFrom::End(-5)).await.unwrap(), 122);
    assert_eq!(buf.read_u8().await.unwrap(), 122);
}

#[tokio::test]
async fn must_seek_pop_error() {
    assert!(AsyncIoCompat::new(FailSeek)
        .seek(SeekFrom::Start(0))
        .await
        .is_err());
}

#[tokio::test]
async fn must_non_blocking_io() {
    let buf: Vec<u8> = (0..127).into_iter().collect();
    let mut src = AsyncIoCompat::new(NonBlockingIO {
        buf: Cursor::new(buf),
        read: 3,
        write: 3,
        flush: 3,
        seek: 3, // retry when poll_complete
    });
    let mut dest = AsyncIoCompat::new(NonBlockingIO {
        buf: Cursor::new(Vec::new()),
        read: 3,
        write: 3,
        flush: 3,
        seek: 1, // direct return when poll_complete
    });

    assert_eq!(src.seek(SeekFrom::Start(1)).await.unwrap(), 1);

    tokio::io::copy(&mut src, &mut dest).await.unwrap();

    assert_eq!(dest.seek(SeekFrom::Start(1)).await.unwrap(), 1);
    assert_eq!(dest.read_u8().await.unwrap(), 2);
}

#[tokio::test]
async fn must_non_blocking_io_with_wait() {
    let buf: Vec<u8> = (0..127).into_iter().collect();
    let mut src = AsyncIoCompat::new_with_delay(
        NonBlockingIO {
            buf: Cursor::new(buf),
            read: 3,
            write: 3,
            flush: 3,
            seek: 3, // retry when poll_complete
        },
        Duration::from_millis(100),
    );
    let mut dest = AsyncIoCompat::new_with_delay(
        NonBlockingIO {
            buf: Cursor::new(Vec::new()),
            read: 3,
            write: 3,
            flush: 3,
            seek: 1, // direct return when poll_complete
        },
        Duration::from_millis(100),
    );

    assert_eq!(src.seek(SeekFrom::Start(1)).await.unwrap(), 1);

    tokio::io::copy(&mut src, &mut dest).await.unwrap();

    assert_eq!(dest.seek(SeekFrom::Start(1)).await.unwrap(), 1);
    assert_eq!(dest.read_u8().await.unwrap(), 2);
}
