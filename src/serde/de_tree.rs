use std::io::{Error, Read, Result, Write};

use sha2::Digest;

use crate::sha2::Sha256;

use super::parse_atom::decode_size_with_offset;
use super::utils::{copy_exactly, skip_bytes};

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

struct ShaWrapper(Sha256);

impl Write for ShaWrapper {
    fn write(&mut self, blob: &[u8]) -> std::result::Result<usize, Error> {
        self.0.update(blob);
        Ok(blob.len())
    }

    fn flush(&mut self) -> std::result::Result<(), Error> {
        Ok(())
    }
}

/// This data structure is used with `parse_triples`, which returns a triple of
/// integer values for each clvm object in a tree. It's a port of python code.
///
/// The deserializer iterates through the blob and caches a triple of
/// integers for each subtree: the first two integers represent the
/// `(start_offset, end_offset)` within the blob that corresponds to the
/// serialization of that object. You can check the contents of
/// `blob[start_offset]` to determine if the object is a pair (in which case
/// that byte is 0xff) or an atom (anything else). For a pair, the third
/// number corresponds to the index of the array that is the "rest" of the
/// pair (the "first" is always this object's index plus one, so we don't
/// need to save that); for an atom, the third number corresponds to an
/// offset of where the atom's binary data is relative to
/// `blob[start_offset]` (so the atom data is at `blob[triple[0] +
/// triple[2]:triple[1]]`).

#[derive(Debug, PartialEq, Eq)]
pub enum ParsedTriple {
    // If `buffer[start] != 0xff`, this is an atom.
    Atom {
        start: u64,
        end: u64,
        atom_offset: u32,
    },

    // Otherwise, it's a pair.
    Pair {
        start: u64,
        end: u64,
        right_index: u32,
    },
}

enum ParseOpRef {
    ParseObj,
    SaveEnd(usize),
    SaveRightIndex(usize),
}

fn sha_blobs(blobs: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for blob in blobs {
        hasher.update(blob);
    }
    hasher.finalize().into()
}

fn tree_hash_for_byte(byte: u8, calculate_tree_hashes: bool) -> Option<[u8; 32]> {
    if calculate_tree_hashes {
        Some(sha_blobs(&[&[1, byte]]))
    } else {
        None
    }
}

fn skip_or_sha_bytes<R: Read>(
    reader: &mut R,
    size: u64,
    calculate_tree_hashes: bool,
) -> Result<Option<[u8; 32]>> {
    if calculate_tree_hashes {
        let mut hasher = Sha256::new();
        hasher.update([1]);

        let mut wrapper = ShaWrapper(hasher);
        copy_exactly(reader, &mut wrapper, size)?;

        Ok(Some(wrapper.0.finalize().into()))
    } else {
        skip_bytes(reader, size)?;
        Ok(None)
    }
}

/// This alternative mechanism of deserialization generates an array of
/// references to each clvm object. A reference contains three values:
/// a start offset within the blob, an end offset, and a third value that
/// is either: an atom offset (relative to the start offset) where the atom
/// data starts (and continues to the end offset); or an index in the array
/// corresponding to the "right" element of the pair (in which case, the
/// "left" element corresponds to the current index + 1).
///
/// Since these values are offsets into the original buffer, that buffer needs
/// to be kept around to get the original atoms.
type ParsedTriplesOutput = (Vec<ParsedTriple>, Option<Vec<[u8; 32]>>);

/// Parse a serialized clvm object tree to an array of `ParsedTriple` objects.
pub fn parse_triples<R: Read>(
    reader: &mut R,
    calculate_tree_hashes: bool,
) -> Result<ParsedTriplesOutput> {
    let mut result = Vec::new();
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
                    let mut byte_array: [u8; 1] = [0];
                    reader.read_exact(&mut byte_array)?;

                    let start = cursor;
                    cursor += 1;

                    let byte = byte_array[0];

                    if byte == CONS_BOX_MARKER {
                        let index = result.len();
                        let new_object = ParsedTriple::Pair {
                            start,
                            end: 0,
                            right_index: 0,
                        };

                        result.push(new_object);

                        if calculate_tree_hashes {
                            tree_hashes.push([0; 32])
                        }

                        op_stack.push(ParseOpRef::SaveEnd(index));
                        op_stack.push(ParseOpRef::ParseObj);
                        op_stack.push(ParseOpRef::SaveRightIndex(index));
                        op_stack.push(ParseOpRef::ParseObj);
                    } else {
                        let (start, end, atom_offset, tree_hash) = {
                            if byte <= MAX_SINGLE_BYTE {
                                (
                                    start,
                                    start + 1,
                                    0,
                                    tree_hash_for_byte(byte, calculate_tree_hashes),
                                )
                            } else {
                                let (atom_offset, atom_size) =
                                    decode_size_with_offset(reader, byte)?;
                                let end = start + (atom_offset as u64) + atom_size;
                                let hash =
                                    skip_or_sha_bytes(reader, atom_size, calculate_tree_hashes)?;

                                (start, end, atom_offset as u32, hash)
                            }
                        };

                        if calculate_tree_hashes {
                            tree_hashes.push(tree_hash.expect("failed unwrap"))
                        }

                        let new_object = ParsedTriple::Atom {
                            start,
                            end,
                            atom_offset,
                        };

                        cursor = end;
                        result.push(new_object);
                    }
                }
                ParseOpRef::SaveEnd(index) => match &mut result[index] {
                    ParsedTriple::Pair {
                        start: _,
                        end,
                        right_index,
                    } => {
                        if calculate_tree_hashes {
                            tree_hashes[index] = sha_blobs(&[
                                &[2],
                                &tree_hashes[index + 1],
                                &tree_hashes[*right_index as usize],
                            ]);
                        }
                        *end = cursor;
                    }
                    _ => {
                        panic!("internal error: SaveEnd")
                    }
                },
                ParseOpRef::SaveRightIndex(index) => {
                    let new_index = result.len() as u32;
                    match &mut result[index] {
                        ParsedTriple::Pair {
                            start: _,
                            end: _,
                            right_index,
                        } => {
                            *right_index = new_index;
                        }
                        _ => {
                            panic!("internal error: SaveRightIndex")
                        }
                    }
                }
            },
        }
    }

    Ok((
        result,
        if calculate_tree_hashes {
            Some(tree_hashes)
        } else {
            None
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex::FromHex;
    use std::io::Cursor;

    fn check_parse_tree(
        hex: &str,
        expected_triple: Vec<ParsedTriple>,
        expected_sha_tree_hex: &str,
    ) {
        let bytes = Vec::from_hex(hex).unwrap();
        println!("{:?}", bytes);
        let mut cursor = Cursor::new(bytes);
        let (parsed_triple, tree_hash) = parse_triples(&mut cursor, false).unwrap();
        assert_eq!(parsed_triple, expected_triple);
        assert_eq!(tree_hash, None);

        let bytes = Vec::from_hex(hex).unwrap();
        let mut cursor = Cursor::new(bytes);
        let (parsed_triple, tree_hash) = parse_triples(&mut cursor, true).unwrap();
        assert_eq!(parsed_triple, expected_triple);

        let expected_hash = Vec::from_hex(expected_sha_tree_hex).unwrap();
        let actual_hash = tree_hash.unwrap()[0].to_vec();
        assert_eq!(expected_hash, actual_hash);
    }

    fn check_sha_blobs(hex: &str, blobs: &[&[u8]]) {
        let expected = Vec::from_hex(hex).unwrap();
        let actual = sha_blobs(blobs);
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_sha_blobs() {
        check_sha_blobs(
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
            &[&[1_u8]],
        );
        check_sha_blobs(
            "9dcf97a184f32623d11a73124ceb99a5709b083721e878a16d78f596718ba7b2",
            &[&[1], &[1]],
        );
        check_sha_blobs(
            "812195e02ed84360ceafab26f9fa6072f8aa76ba34a735894c3f3c2e4fe6911d",
            &[&[1, 250, 17], &[28]],
        );
    }

    #[test]
    fn test_parse_tree() {
        check_parse_tree(
            "80",
            vec![ParsedTriple::Atom {
                start: 0,
                end: 1,
                atom_offset: 1,
            }],
            "4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a",
        );

        check_parse_tree(
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
            "247f7d3f63b346ea93ca47f571cd0f4455392348b888a4286072bef0ac6069b5",
        );

        check_parse_tree(
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
            "47f30bf9935e25e4262023124fb5e986d755b9ed65a28ac78925c933bfd57dbd",
        );

        let hex = "c0a0".to_owned() + &hex::encode([0x31u8; 160]);
        check_parse_tree(
            &hex,
            vec![ParsedTriple::Atom {
                start: 0,
                end: 162,
                atom_offset: 2,
            }],
            "d1c109981a9c5a3bbe2d98795a186a0f057dc9a3a7f5e1eb4dfb63a1636efa2d",
        );
    }
}
