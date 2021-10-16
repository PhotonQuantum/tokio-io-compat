use std::io::Read;
use std::io::{Cursor, ErrorKind, Seek};

use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use crate::AsyncIoCompat;

struct ChunkedRead<T>(T, usize);

impl<T: Read> Read for ChunkedRead<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        assert!(buf.len() > self.1);
        let mut chunk = vec![0; self.1];
        let size = self.0.read(&mut chunk).unwrap();
        buf[..self.1].copy_from_slice(&*chunk);
        Ok(size)
    }
}

struct FailSeek;

impl Seek for FailSeek {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::new(ErrorKind::Other, "boom"))
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
