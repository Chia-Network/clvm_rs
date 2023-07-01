use std::io;
use std::io::{Cursor, Read, Seek, SeekFrom};

use super::errors::bad_encoding;
use super::parse_atom::decode_size;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

#[repr(u8)]
enum ParseOp {
    SExp,
    Cons,
}

pub fn serialized_length_from_bytes(bytes: &[u8]) -> io::Result<u64> {
    let mut cursor = Cursor::new(bytes);
    let mut ops = vec![ParseOp::SExp];
    let mut bytes = [0; 1];

    loop {
        let op = ops.pop();

        if op.is_none() {
            break;
        }

        match op.unwrap() {
            ParseOp::SExp => {
                cursor.read_exact(&mut bytes)?;
                if bytes[0] == CONS_BOX_MARKER {
                    // since all we're doing is to determing the length of the
                    // serialized buffer, we don't need to do anything about
                    // "cons". So we skip pushing it to lower the pressure on
                    // the op stack
                    //ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if bytes[0] == 0x80 || bytes[0] <= MAX_SINGLE_BYTE {
                    // This one byte we just read was the whole atom.
                    // or the
                    // special case of NIL
                } else {
                    let blob_size = decode_size(&mut cursor, bytes[0])?;
                    cursor.seek(SeekFrom::Current(blob_size as i64))?;
                    if (cursor.get_ref().len() as u64) < cursor.position() {
                        return Err(bad_encoding());
                    }
                }
            }
            ParseOp::Cons => {
                // cons. No need to construct any structure here. Just keep
                // going
            }
        }
    }

    Ok(cursor.position())
}

use crate::sha2::{Digest, Sha256};

fn hash_atom(buf: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([1_u8]);
    ctx.update(buf);
    ctx.finalize().into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([2_u8]);
    ctx.update(left);
    ctx.update(right);
    ctx.finalize().into()
}

// computes the tree-hash of a CLVM structure in serialized form
pub fn tree_hash_from_stream(cursor: &mut Cursor<&[u8]>) -> io::Result<[u8; 32]> {
    let mut values: Vec<[u8; 32]> = Vec::new();
    let mut ops = vec![ParseOp::SExp];
    let mut byte = [0; 1];

    loop {
        let op = ops.pop();
        if op.is_none() {
            break;
        }
        match op.unwrap() {
            ParseOp::SExp => {
                cursor.read_exact(&mut byte)?;
                if byte[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if byte[0] == 0x80 {
                    values.push(hash_atom(&[]));
                } else if byte[0] <= MAX_SINGLE_BYTE {
                    values.push(hash_atom(&byte));
                } else {
                    let blob_size = decode_size(cursor, byte[0])?;
                    let blob = &cursor.get_ref()[cursor.position() as usize..];
                    if (blob.len() as u64) < blob_size {
                        return Err(bad_encoding());
                    }
                    cursor.set_position(cursor.position() + blob_size);
                    values.push(hash_atom(&blob[..blob_size as usize]));
                }
            }
            ParseOp::Cons => {
                let value_2 = values.pop();
                let value_1 = values.pop();
                values.push(hash_pair(&value_1.unwrap(), &value_2.unwrap()));
            }
        }
    }

    Ok(values.pop().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    use hex::FromHex;

    #[test]
    fn test_tree_hash_max_single_byte() {
        let mut ctx = Sha256::new();
        ctx.update([1_u8]);
        ctx.update([0x7f_u8]);
        let mut cursor = Cursor::<&[u8]>::new(&[0x7f_u8]);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap(),
            ctx.finalize().as_slice()
        );
    }

    #[test]
    fn test_tree_hash_one() {
        let mut ctx = Sha256::new();
        ctx.update([1_u8]);
        ctx.update([1_u8]);
        let mut cursor = Cursor::<&[u8]>::new(&[1_u8]);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap(),
            ctx.finalize().as_slice()
        );
    }

    #[test]
    fn test_tree_hash_zero() {
        let mut ctx = Sha256::new();
        ctx.update([1_u8]);
        ctx.update([0_u8]);
        let mut cursor = Cursor::<&[u8]>::new(&[0_u8]);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap(),
            ctx.finalize().as_slice()
        );
    }

    #[test]
    fn test_tree_hash_nil() {
        let mut ctx = Sha256::new();
        ctx.update([1_u8]);
        let mut cursor = Cursor::<&[u8]>::new(&[0x80_u8]);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap(),
            ctx.finalize().as_slice()
        );
    }

    #[test]
    fn test_tree_hash_overlong() {
        let mut cursor = Cursor::<&[u8]>::new(&[0x8f, 0xff]);
        let e = tree_hash_from_stream(&mut cursor).unwrap_err();
        assert_eq!(e.kind(), bad_encoding().kind());

        let mut cursor = Cursor::<&[u8]>::new(&[0b11001111, 0xff]);
        let e = tree_hash_from_stream(&mut cursor).unwrap_err();
        assert_eq!(e.kind(), bad_encoding().kind());

        let mut cursor = Cursor::<&[u8]>::new(&[0b11001111, 0xff, 0, 0]);
        let e = tree_hash_from_stream(&mut cursor).unwrap_err();
        assert_eq!(e.kind(), bad_encoding().kind());
    }

    // these test cases were produced by:

    // from chia.types.blockchain_format.program import Program
    // a = Program.to(...)
    // print(bytes(a).hex())
    // print(a.get_tree_hash().hex())

    #[test]
    fn test_tree_hash_list() {
        // this is the list (1 (2 (3 (4 (5 ())))))
        let buf = Vec::from_hex("ff01ff02ff03ff04ff0580").unwrap();
        let mut cursor = Cursor::<&[u8]>::new(&buf);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
            Vec::from_hex("123190dddde51acfc61f48429a879a7b905d1726a52991f7d63349863d06b1b6")
                .unwrap()
        );
    }

    #[test]
    fn test_tree_hash_tree() {
        // this is the tree ((1, 2), (3, 4))
        let buf = Vec::from_hex("ffff0102ff0304").unwrap();
        let mut cursor = Cursor::<&[u8]>::new(&buf);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
            Vec::from_hex("2824018d148bc6aed0847e2c86aaa8a5407b916169f15b12cea31fa932fc4c8d")
                .unwrap()
        );
    }

    #[test]
    fn test_tree_hash_tree_large_atom() {
        // this is the tree ((1, 2), (3, b"foobar"))
        let buf = Vec::from_hex("ffff0102ff0386666f6f626172").unwrap();
        let mut cursor = Cursor::<&[u8]>::new(&buf);
        assert_eq!(
            tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
            Vec::from_hex("b28d5b401bd02b65b7ed93de8e916cfc488738323e568bcca7e032c3a97a12e4")
                .unwrap()
        );
    }

    #[test]
    fn test_serialized_length_from_bytes() {
        assert_eq!(
            serialized_length_from_bytes(&[0x7f, 0x00, 0x00, 0x00]).unwrap(),
            1
        );
        assert_eq!(
            serialized_length_from_bytes(&[0x80, 0x00, 0x00, 0x00]).unwrap(),
            1
        );
        assert_eq!(
            serialized_length_from_bytes(&[0xff, 0x00, 0x00, 0x00]).unwrap(),
            3
        );
        assert_eq!(
            serialized_length_from_bytes(&[0xff, 0x01, 0xff, 0x80, 0x80, 0x00]).unwrap(),
            5
        );

        let e = serialized_length_from_bytes(&[0x8f, 0xff]).unwrap_err();
        assert_eq!(e.kind(), bad_encoding().kind());
        assert_eq!(e.to_string(), "bad encoding");

        let e = serialized_length_from_bytes(&[0b11001111, 0xff]).unwrap_err();
        assert_eq!(e.kind(), bad_encoding().kind());
        assert_eq!(e.to_string(), "bad encoding");

        let e = serialized_length_from_bytes(&[0b11001111, 0xff, 0, 0]).unwrap_err();
        assert_eq!(e.kind(), bad_encoding().kind());
        assert_eq!(e.to_string(), "bad encoding");

        assert_eq!(
            serialized_length_from_bytes(&[0x8f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
                .unwrap(),
            16
        );
    }
}
