use std::io;
use std::io::Cursor;
use std::io::ErrorKind;
use std::io::Write;

use super::write_atom::write_atom;
use crate::allocator::{len_for_value, Allocator, NodePtr, NodeVisitor};
use crate::error::Result;

const CONS_BOX_MARKER: u8 = 0xff;

pub struct LimitedWriter<W: Write> {
    inner: W,
    limit: usize,
}

impl<W: Write> LimitedWriter<W> {
    pub fn new(w: W, limit: usize) -> LimitedWriter<W> {
        LimitedWriter { inner: w, limit }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.limit < buf.len() {
            return Err(ErrorKind::OutOfMemory.into());
        }
        let written = self.inner.write(buf)?;
        self.limit -= written;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
/// serialize a node
pub fn node_to_stream<W: Write>(a: &Allocator, node: NodePtr, f: &mut W) -> Result<()> {
    let mut values: Vec<NodePtr> = vec![node];
    while let Some(v) = values.pop() {
        match a.node(v) {
            NodeVisitor::Buffer(buf) => write_atom(f, buf)?,
            NodeVisitor::U32(val) => {
                let buf = val.to_be_bytes();
                let len = len_for_value(val);
                write_atom(f, &buf[4 - len..])?
            }
            NodeVisitor::Pair(left, right) => {
                f.write_all(&[CONS_BOX_MARKER])?;
                values.push(right);
                values.push(left);
            }
        }
    }
    Ok(())
}

pub fn node_to_bytes_limit(a: &Allocator, node: NodePtr, limit: usize) -> Result<Vec<u8>> {
    let buffer = Cursor::new(Vec::new());
    let mut writer = LimitedWriter::new(buffer, limit);
    node_to_stream(a, node, &mut writer)?;
    let vec = writer.into_inner().into_inner();
    Ok(vec)
}

pub fn node_to_bytes(a: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    node_to_bytes_limit(a, node, 2000000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EvalErr;

    #[test]
    fn test_serialize_limit() {
        let mut a = Allocator::new();

        let leaf = a.new_atom(&[1, 2, 3, 4, 5]).unwrap();
        let l1 = a.new_pair(leaf, leaf).unwrap();
        let l2 = a.new_pair(l1, l1).unwrap();
        let l3 = a.new_pair(l2, l2).unwrap();

        {
            let buffer = Cursor::new(Vec::new());
            let mut writer = LimitedWriter::new(buffer, 55);
            node_to_stream(&a, l3, &mut writer).unwrap();
            let vec = writer.into_inner().into_inner();
            assert_eq!(
                vec,
                &[
                    0xff, 0xff, 0xff, 133, 1, 2, 3, 4, 5, 133, 1, 2, 3, 4, 5, 0xff, 133, 1, 2, 3,
                    4, 5, 133, 1, 2, 3, 4, 5, 0xff, 0xff, 133, 1, 2, 3, 4, 5, 133, 1, 2, 3, 4, 5,
                    0xff, 133, 1, 2, 3, 4, 5, 133, 1, 2, 3, 4, 5
                ]
            );
        }

        {
            let buffer = Cursor::new(Vec::new());
            let mut writer = LimitedWriter::new(buffer, 54);
            assert_eq!(
                node_to_stream(&a, l3, &mut writer).unwrap_err(),
                EvalErr::OutOfMemory
            );
        }
    }
}
