use std::io;
use std::io::{ErrorKind, Read};

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

fn bad_encoding() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "bad encoding")
}

fn internal_error() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "internal error")
}

/// This data structure is used with `parse_triples`, which returns a triple of
/// integer values for each clvm object in a tree.

#[derive(Debug, PartialEq, Eq)]
pub enum ParsedTriple {
    Atom {
        start: u64,
        end: u64,
        atom_offset: u32,
    },
    Pair {
        start: u64,
        end: u64,
        right_index: u32,
    },
}

enum ParseOpRef {
    ParseObj,
    SaveCursor(usize),
    SaveIndex(usize),
}

fn skip_bytes<R: io::Read>(f: &mut R, skip_size: u64) -> io::Result<u64> {
    io::copy(&mut f.by_ref().take(skip_size), &mut io::sink())
}

/// parse a serialized clvm object tree to an array of `ParsedTriple` objects

/// This alternative mechanism of deserialization generates an array of
/// references to each clvm object. A reference contains three values:
/// a start offset within the blob, an end offset, and a third value that
/// is either: an atom offset (relative to the start offset) where the atom
/// data starts (and continues to the end offset); or an index in the array
/// corresponding to the "right" element of the pair (in which case, the
/// "left" element corresponds to the next index in the array).
///
/// Since these values are offsets into the original buffer, that buffer needs
/// to be kept around to get the original atoms.

pub fn parse_triples<R: io::Read>(f: &mut R) -> io::Result<Vec<ParsedTriple>> {
    let mut r = Vec::new();
    let mut op_stack = vec![ParseOpRef::ParseObj];
    let mut cursor: u64 = 0;
    loop {
        match op_stack.pop() {
            None => {
                break;
            }
            Some(op) => match op {
                ParseOpRef::ParseObj => {
                    let mut b: [u8; 1] = [0];
                    f.read_exact(&mut b)?;
                    let start = cursor;
                    cursor += 1;
                    let b = b[0];
                    if b == CONS_BOX_MARKER {
                        let index = r.len();
                        let new_obj = ParsedTriple::Pair {
                            start,
                            end: 0,
                            right_index: 0,
                        };
                        r.push(new_obj);
                        op_stack.push(ParseOpRef::SaveCursor(index));
                        op_stack.push(ParseOpRef::ParseObj);
                        op_stack.push(ParseOpRef::SaveIndex(index));
                        op_stack.push(ParseOpRef::ParseObj);
                    } else {
                        let (start, end, atom_offset) = {
                            if b <= MAX_SINGLE_BYTE {
                                (start, start + 1, 0)
                            } else {
                                let (atom_offset, atom_size) = decode_size(f, b)?;
                                skip_bytes(f, atom_size)?;
                                let end = start + (atom_offset as u64) + (atom_size);
                                (start, end, atom_offset as u32)
                            }
                        };
                        let new_obj = ParsedTriple::Atom {
                            start,
                            end,
                            atom_offset,
                        };
                        cursor = end;
                        r.push(new_obj);
                    }
                }
                ParseOpRef::SaveCursor(index) => {
                    if let ParsedTriple::Pair {
                        start,
                        end: _,
                        right_index,
                    } = r[index]
                    {
                        r[index] = ParsedTriple::Pair {
                            start,
                            end: cursor,
                            right_index,
                        };
                    }
                }
                ParseOpRef::SaveIndex(index) => {
                    if let ParsedTriple::Pair {
                        start,
                        end,
                        right_index: _,
                    } = r[index]
                    {
                        r[index] = ParsedTriple::Pair {
                            start,
                            end,
                            right_index: r.len() as u32,
                        };
                    }
                }
            },
        }
    }
    Ok(r)
}

/// decode the length prefix for an atom. Atoms whose value fit in 7 bits
/// don't have a length prefix, so those should be handled specially and
/// never passed to this function.
fn decode_size<R: io::Read>(f: &mut R, initial_b: u8) -> io::Result<(u8, u64)> {
    debug_assert!((initial_b & 0x80) != 0);
    if (initial_b & 0x80) == 0 {
        return Err(internal_error());
    }

    let mut atom_start_offset = 0;
    let mut bit_mask: u8 = 0x80;
    let mut b = initial_b;
    while b & bit_mask != 0 {
        atom_start_offset += 1;
        b &= 0xff ^ bit_mask;
        bit_mask >>= 1;
    }
    let mut size_blob: Vec<u8> = Vec::new();
    size_blob.resize(atom_start_offset, 0);
    size_blob[0] = b;
    if atom_start_offset > 1 {
        let remaining_buffer = &mut size_blob[1..atom_start_offset];
        f.read_exact(remaining_buffer)?;
    }
    // need to convert size_blob to an int
    let mut atom_size: u64 = 0;
    if size_blob.len() > 6 {
        return Err(bad_encoding());
    }
    for b in &size_blob {
        atom_size <<= 8;
        atom_size += *b as u64;
    }
    if atom_size >= 0x400000000 {
        return Err(bad_encoding());
    }
    Ok((atom_start_offset as u8, atom_size))
}

#[cfg(test)]
use hex::FromHex;

#[cfg(test)]
use std::io::Cursor;

#[cfg(test)]
fn check_parse_triple(h: &str, expected: Vec<ParsedTriple>) -> () {
    let b = Vec::from_hex(h).unwrap();
    println!("{:?}", b);
    let mut f = Cursor::new(b);
    let p = parse_triples(&mut f).unwrap();
    assert_eq!(p, expected);
}

#[test]
fn test_parse_triple() {
    check_parse_triple(
        "80",
        vec![ParsedTriple::Atom {
            start: 0,
            end: 1,
            atom_offset: 1,
        }],
    );

    check_parse_triple(
        "ff648200c8",
        vec![
            ParsedTriple::Pair {
                start: 0,
                end: 5,
                right_index: 2,
            },
            ParsedTriple::Atom {
                start: 1,
                end: 2,
                atom_offset: 0,
            },
            ParsedTriple::Atom {
                start: 2,
                end: 5,
                atom_offset: 1,
            },
        ],
    );

    check_parse_triple(
        "ff83666f6fff83626172ff8362617a80", // `(foo bar baz)`
        vec![
            ParsedTriple::Pair {
                start: 0,
                end: 16,
                right_index: 2,
            },
            ParsedTriple::Atom {
                start: 1,
                end: 5,
                atom_offset: 1,
            },
            ParsedTriple::Pair {
                start: 5,
                end: 16,
                right_index: 4,
            },
            ParsedTriple::Atom {
                start: 6,
                end: 10,
                atom_offset: 1,
            },
            ParsedTriple::Pair {
                start: 10,
                end: 16,
                right_index: 6,
            },
            ParsedTriple::Atom {
                start: 11,
                end: 15,
                atom_offset: 1,
            },
            ParsedTriple::Atom {
                start: 15,
                end: 16,
                atom_offset: 1,
            },
        ],
    );

    check_parse_triple(
        "c0a03131313131313131313131313131313131313131313131313131313131313131313131313131\
         31313131313131313131313131313131313131313131313131313131313131313131313131313131\
         31313131313131313131313131313131313131313131313131313131313131313131313131313131\
         313131313131313131313131313131313131313131313131313131313131313131313131313131313131",
        vec![ParsedTriple::Atom {
            start: 0,
            end: 162,
            atom_offset: 2,
        }],
    );
}
