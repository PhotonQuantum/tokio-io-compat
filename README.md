# tokio-io-compat

[![GitHub Workflow Status](https://img.shields.io/github/workflow/status/PhotonQuantum/tokio-io-compat/Test?style=flat-square)](https://github.com/PhotonQuantum/tokio-io-compat/actions/workflows/test.yml)
[![crates.io](https://img.shields.io/crates/v/tokio-io-compat?style=flat-square)](https://crates.io/crates/tokio-io-compat)
[![Documentation](https://img.shields.io/docsrs/tokio-io-compat?style=flat-square)](https://docs.rs/tokio-io-compat)

Compatibility wrapper around `std::io::{Read, Write, Seek}` traits that
implements `tokio::io::{AsyncRead, AsyncWrite, AsyncSeek}`.

Beware that this won't magically make your IO operations asynchronous.
You should still consider asyncify your code or move the IO operations to blocking thread if the cost is high.


## Deal with `WouldBlock`

If you are trying to wrap a non-blocking IO, it may yield `WouldBlock` errors when data
is not ready.
This wrapper will automatically convert `WouldBlock` into `Poll::Pending`.

However, the waker must be waken later to avoid blocking the future.
By default, it is waken immediately. This may waste excessive CPU cycles, especially when the operation
is slow.

You may add a delay before each wake by creating the wrapper with `AsyncIoCompat::new_with_delay`.
If your underlying non-blocking IO has a native poll complete notification mechanism, consider
writing your own wrapper instead of using this crate.

For reference please see [tokio-tls](https://github.com/tokio-rs/tls/blob/master/tokio-native-tls/src/lib.rs).

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