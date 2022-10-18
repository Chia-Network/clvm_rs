use std::convert::TryInto;
use std::io;
use std::io::{Read, Write};

use sha2::Digest;

use crate::sha2::Sha256;

use crate::serde::decode_size;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

struct ShaWrapper(Sha256);

impl Write for ShaWrapper {
    fn write(&mut self, blob: &[u8]) -> std::result::Result<usize, std::io::Error> {
        self.0.update(blob);
        Ok(blob.len())
    }
    fn flush(&mut self) -> std::result::Result<(), std::io::Error> {
        Ok(())
    }
}

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

fn sha_blobs(blobs: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for blob in blobs {
        h.update(blob);
    }
    h.finalize()
        .as_slice()
        .try_into()
        .expect("wrong slice length")
}

fn tree_hash_for_byte(b: u8, calculate_tree_hashes: bool) -> Option<[u8; 32]> {
    if calculate_tree_hashes {
        Some(sha_blobs(&[&[1, b]]))
    } else {
        None
    }
}

fn skip_bytes<R: io::Read>(f: &mut R, skip_size: u64) -> io::Result<u64> {
    io::copy(&mut f.by_ref().take(skip_size), &mut io::sink())
}

fn skip_or_sha_bytes<R: io::Read>(
    f: &mut R,
    skip_size: u64,
    calculate_tree_hashes: bool,
) -> io::Result<Option<[u8; 32]>> {
    if calculate_tree_hashes {
        let mut f = &mut f.by_ref().take(skip_size);
        let mut h = Sha256::new();
        h.update(&[1]);
        let mut w = ShaWrapper(h);
        io::copy(&mut f, &mut w)?;
        let r: [u8; 32] =
            w.0.finalize()
                .as_slice()
                .try_into()
                .expect("wrong slice length");
        Ok(Some(r))
    } else {
        skip_bytes(f, skip_size)?;
        Ok(None)
    }
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

pub fn deserialize_tree<R: io::Read>(
    f: &mut R,
    calculate_tree_hashes: bool,
) -> io::Result<(Vec<CLVMTreeBoundary>, Option<Vec<[u8; 32]>>)> {
    let mut r = Vec::new();
    let mut tree_hashes = Vec::new();
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
                        if calculate_tree_hashes {
                            tree_hashes.push([0; 32])
                        }
                        op_stack.push(ParseOpRef::SaveCursor(index));
                        op_stack.push(ParseOpRef::ParseObj);
                        op_stack.push(ParseOpRef::SaveIndex(index));
                        op_stack.push(ParseOpRef::ParseObj);
                    } else {
                        let (start, end, atom_offset, tree_hash) = {
                            if b <= MAX_SINGLE_BYTE {
                                (
                                    start,
                                    start + 1,
                                    0,
                                    tree_hash_for_byte(b, calculate_tree_hashes),
                                )
                            } else {
                                let (atom_offset, atom_size) = decode_size(f, b)?;
                                let end = start + (atom_offset as u64) + (atom_size as u64);
                                let h = skip_or_sha_bytes(f, atom_size, calculate_tree_hashes)?;
                                (start, end, atom_offset as u32, h)
                            }
                        };
                        if calculate_tree_hashes {
                            tree_hashes.push(tree_hash.expect("failed unwrap"))
                        }
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
                        if calculate_tree_hashes {
                            let h = sha_blobs(&[
                                &[2],
                                &tree_hashes[index + 1],
                                &tree_hashes[right_index as usize],
                            ]);
                            tree_hashes[index] = h;
                        }
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
    Ok((
        r,
        if calculate_tree_hashes {
            Some(tree_hashes)
        } else {
            None
        },
    ))
}

#[cfg(test)]
use std::io::Cursor;

#[cfg(test)]
use hex::FromHex;

#[cfg(test)]
fn check_parse_tree(h: &str, expected: Vec<CLVMTreeBoundary>, expected_sha_tree_hex: &str) -> () {
    let b = Vec::from_hex(h).unwrap();
    println!("{:?}", b);
    let mut f = Cursor::new(b);
    let (p, tree_hash) = deserialize_tree(&mut f, false).unwrap();
    assert_eq!(p, expected);
    assert_eq!(tree_hash, None);

    let b = Vec::from_hex(h).unwrap();
    let mut f = Cursor::new(b);
    let (p, tree_hash) = deserialize_tree(&mut f, true).unwrap();
    assert_eq!(p, expected);

    let est = Vec::from_hex(expected_sha_tree_hex).unwrap();
    assert_eq!(tree_hash.unwrap()[0].to_vec(), est);
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
        "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
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
        "247f7d3f63b346ea93ca47f571cd0f4455392348b888a4286072bef0ac6069b5",
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
        "47f30bf9935e25e4262023124fb5e986d755b9ed65a28ac78925c933bfd57dbd",
    );

    let s = "c0a0".to_owned() + &hex::encode([0x31u8; 160]);
    check_parse_tree(
        &s,
        vec![CLVMTreeBoundary::Atom {
            start: 0,
            end: 162,
            atom_offset: 2,
        }],
        "d1c109981a9c5a3bbe2d98795a186a0f057dc9a3a7f5e1eb4dfb63a1636efa2d",
    );
}
