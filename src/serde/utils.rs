use crate::error::{EvalErr, Result};
use crate::NodePtr;
use std::io::{copy, sink, Read, Write};

pub fn copy_exactly<R: Read, W: ?Sized + Write>(
    reader: &mut R,
    writer: &mut W,
    expected_size: u64,
) -> Result<()> {
    let mut reader = reader.by_ref().take(expected_size);

    let count = copy(&mut reader, writer)?;
    if count < expected_size {
        Err(EvalErr::InternalError(
            NodePtr::NIL,
            "copy terminated early".to_string(),
        ))?
    } else {
        Ok(())
    }
}

pub fn skip_bytes<R: Read>(f: &mut R, size: u64) -> Result<()> {
    copy_exactly(f, &mut sink(), size)
}
