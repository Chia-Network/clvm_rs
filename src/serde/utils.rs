use std::io;
use std::io::{copy, sink, Error, Read, Write};

pub fn copy_exactly<R: Read, W: ?Sized + Write>(
    reader: &mut R,
    writer: &mut W,
    expected_size: u64,
) -> io::Result<()> {
    let mut reader = reader.by_ref().take(expected_size);

    let count = copy(&mut reader, writer)?;
    if count < expected_size {
        Err(Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "copy terminated early",
        ))
    } else {
        Ok(())
    }
}

pub fn skip_bytes<R: Read>(f: &mut R, size: u64) -> io::Result<()> {
    copy_exactly(f, &mut sink(), size)
}
