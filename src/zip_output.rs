use std::io::{self, Seek, SeekFrom, Write};
/// The ZIP library finalizes in Drop and logs if that fails. After the first
/// real I/O error, only its cleanup sees a simulated sink; the facade always
/// returns the latched original error and can never report a successful file.
pub(crate) struct DropSafeWriter<W> {
    pub(crate) inner: W,
    pub(crate) failed: std::rc::Rc<std::cell::Cell<Option<io::ErrorKind>>>,
    pub(crate) position: u64,
    pub(crate) length: u64,
}
impl<W: Write + Seek> Write for DropSafeWriter<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = if self.failed.get().is_some() {
            b.len()
        } else {
            match self.inner.write(b) {
                Ok(n) => n,
                Err(e) => {
                    self.failed.set(Some(e.kind()));
                    return Err(io::Error::from(e.kind()));
                }
            }
        };
        self.position += n as u64;
        self.length = self.length.max(self.position);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed.get().is_some() {
            return Ok(());
        };
        self.inner.flush().map_err(|e| {
            self.failed.set(Some(e.kind()));
            io::Error::from(e.kind())
        })
    }
}
impl<W: Write + Seek> Seek for DropSafeWriter<W> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        if self.failed.get().is_none() {
            match self.inner.seek(p) {
                Ok(n) => {
                    self.position = n;
                    return Ok(n);
                }
                Err(e) => {
                    self.failed.set(Some(e.kind()));
                    return Err(io::Error::from(e.kind()));
                }
            }
        }
        let n = match p {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => i128::from(self.length) + i128::from(n),
        };
        self.position =
            u64::try_from(n).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        Ok(self.position)
    }
}
