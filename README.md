# tokio-io-compat

[![GitHub Workflow Status](https://img.shields.io/github/workflow/status/PhotonQuantum/tokio-io-compat/Test?style=flat-square)](https://github.com/PhotonQuantum/tokio-io-compat/actions/workflows/test.yml)
[![crates.io](https://img.shields.io/crates/v/tokio-io-compat?style=flat-square)](https://crates.io/crates/tokio-io-compat)
[![Documentation](https://img.shields.io/docsrs/tokio-io-compat?style=flat-square)](https://docs.rs/tokio-io-compat)

Compatibility wrapper around `std::io::{Read, Write, Seek}` traits that
implements `tokio::io::{AsyncRead, AsyncWrite, AsyncSeek}`.

Beware that this won't magically make your IO operations asynchronous.
You should still consider asyncify your code or move the IO operations to blocking thread if the cost is high.

## Example

```rust
use std::io::Cursor;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tokio_io_compat::CompatHelperTrait;

let mut data = Cursor::new(vec![]);
data.tokio_io_mut().write_all(&vec![0, 1, 2, 3, 4]).await.unwrap();
data.tokio_io_mut().seek(SeekFrom::Start(2)).await.unwrap();
assert_eq!(data.tokio_io_mut().read_u8().await.unwrap(), 2);
```