use std::io;
use std::io::Read;

use crate::serde::decode_size;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

/// This data structure is used with `deserialize_tree`, which returns a triple of
/// integer values for each clvm object in a tree.

#[derive(Debug, PartialEq, Eq)]
pub enum CLVMTreeBoundary {
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

/// parse a serialized clvm object tree to an array of `CLVMTreeBoundary` objects

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

pub fn deserialize_tree<R: io::Read>(f: &mut R) -> io::Result<Vec<CLVMTreeBoundary>> {
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
                    let start = cursor as u64;
                    cursor += 1;
                    let b = b[0];
                    if b == CONS_BOX_MARKER {
                        let index = r.len();
                        let new_obj = CLVMTreeBoundary::Pair {
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
                                let end = start + (atom_offset as u64) + (atom_size as u64);
                                (start, end, atom_offset as u32)
                            }
                        };
                        let new_obj = CLVMTreeBoundary::Atom {
                            start,
                            end,
                            atom_offset,
                        };
                        cursor = end;
                        r.push(new_obj);
                    }
                }
                ParseOpRef::SaveCursor(index) => {
                    if let CLVMTreeBoundary::Pair {
                        start,
                        end: _,
                        right_index,
                    } = r[index]
                    {
                        r[index] = CLVMTreeBoundary::Pair {
                            start,
                            end: cursor,
                            right_index,
                        };
                    }
                }
                ParseOpRef::SaveIndex(index) => {
                    if let CLVMTreeBoundary::Pair {
                        start,
                        end,
                        right_index: _,
                    } = r[index]
                    {
                        r[index] = CLVMTreeBoundary::Pair {
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

#[cfg(test)]
use std::io::Cursor;

#[cfg(test)]
use hex::FromHex;

#[cfg(test)]
fn check_parse_tree(h: &str, expected: Vec<CLVMTreeBoundary>) -> () {
    let b = Vec::from_hex(h).unwrap();
    println!("{:?}", b);
    let mut f = Cursor::new(b);
    let p = deserialize_tree(&mut f).unwrap();
    assert_eq!(p, expected);
}

#[test]
fn test_parse_tree() {
    check_parse_tree(
        "80",
        vec![CLVMTreeBoundary::Atom {
            start: 0,
            end: 1,
            atom_offset: 1,
        }],
    );

    check_parse_tree(
        "ff648200c8",
        vec![
            CLVMTreeBoundary::Pair {
                start: 0,
                end: 5,
                right_index: 2,
            },
            CLVMTreeBoundary::Atom {
                start: 1,
                end: 2,
                atom_offset: 0,
            },
            CLVMTreeBoundary::Atom {
                start: 2,
                end: 5,
                atom_offset: 1,
            },
        ],
    );

    check_parse_tree(
        "ff83666f6fff83626172ff8362617a80", // `(foo bar baz)`
        vec![
            CLVMTreeBoundary::Pair {
                start: 0,
                end: 16,
                right_index: 2,
            },
            CLVMTreeBoundary::Atom {
                start: 1,
                end: 5,
                atom_offset: 1,
            },
            CLVMTreeBoundary::Pair {
                start: 5,
                end: 16,
                right_index: 4,
            },
            CLVMTreeBoundary::Atom {
                start: 6,
                end: 10,
                atom_offset: 1,
            },
            CLVMTreeBoundary::Pair {
                start: 10,
                end: 16,
                right_index: 6,
            },
            CLVMTreeBoundary::Atom {
                start: 11,
                end: 15,
                atom_offset: 1,
            },
            CLVMTreeBoundary::Atom {
                start: 15,
                end: 16,
                atom_offset: 1,
            },
        ],
    );

    let s = "c0a0".to_owned() + &hex::encode([0x31u8; 160]);
    check_parse_tree(
        &s,
        vec![CLVMTreeBoundary::Atom {
            start: 0,
            end: 162,
            atom_offset: 2,
        }],
    );
}
