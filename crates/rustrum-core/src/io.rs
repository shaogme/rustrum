use crate::error::Error;
use alloc::{string::String, vec::Vec};

pub trait Write {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Error>;
}

pub trait Read {
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error>;
}

impl<W: Write + ?Sized> Write for &mut W {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Error> {
        (**self).write_all(buf)
    }
}

impl<R: Read + ?Sized> Read for &mut R {
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        (**self).read_exact(buf)
    }
}

impl Write for Vec<u8> {
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Error> {
        self.extend_from_slice(buf);
        Ok(())
    }
}

impl Read for &[u8] {
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error> {
        if buf.len() > self.len() {
            return Err(Error::Io(String::from("unexpected EOF")));
        }
        let (left, right) = self.split_at(buf.len());
        buf.copy_from_slice(left);
        *self = right;
        Ok(())
    }
}
