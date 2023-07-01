mod bytes32;
mod de;
mod de_br;
mod de_tree;
mod errors;
mod object_cache;
mod parse_atom;
mod read_cache_lookup;
mod ser;
mod ser_br;
mod tools;
mod utils;
pub mod write_atom;

pub use de::node_from_bytes;
pub use de_br::node_from_bytes_backrefs;
pub use de_tree::{parse_triples, ParsedTriple};
pub use ser::node_to_bytes;
pub use ser_br::node_to_bytes_backrefs;
pub use tools::{serialized_length_from_bytes, tree_hash_from_stream};

#[cfg(test)]
mod tests {
    use crate::Allocator;

    use super::*;

    use hex::FromHex;

    fn check(serialized_backrefs_hex: &str) {
        // Serialized with br => obj => serialized no br =(allow_br)=> obj => serialized w br.

        // Serialized object, with back-refs.
        let serialized_backrefs_1 = <Vec<u8>>::from_hex(serialized_backrefs_hex).unwrap();

        // Turn into serialized object with no back-refs.
        let mut allocator = Allocator::new();
        let object = node_from_bytes_backrefs(&mut allocator, &serialized_backrefs_1).unwrap();

        let serialized_1 = node_to_bytes(&allocator, object).unwrap();

        // Deserialize using `node_from_bytes_backrefs` (even though there are no backrefs)
        // and reserialized without back-refs.
        let mut allocator = Allocator::new();
        let object = node_from_bytes_backrefs(&mut allocator, &serialized_1).unwrap();

        let serialized_2 = node_to_bytes(&allocator, object).unwrap();

        // Compare both reserializations (without back-refs).
        assert_eq!(serialized_1, serialized_2);

        // Now reserialize with back-refs.
        let mut allocator = Allocator::new();
        let obj = node_from_bytes(&mut allocator, &serialized_1).unwrap();

        let serialized_backrefs_2 = node_to_bytes_backrefs(&allocator, obj).unwrap();

        // And compare to original.
        assert_eq!(serialized_backrefs_1, serialized_backrefs_2);
    }

    #[test]
    fn test_round_trip() {
        check("01"); // 1
        check("ff83666f6f83626172"); // (foo . bar)
        check("ff83666f6fff8362617280"); // (foo bar)
        check("ffff0102ff0304"); // ((1 . 2) . (3 . 4))
        check("ff01ff02ff03ff04ff05ff0680"); // (1 2 3 4 5 6)
        check("ff83666f6ffe02"); // (foo . foo)

        // (long string of long text string)
        check("ff846c6f6e67ff86737472696e67ff826f66fffe0bff8474657874fffe1780");

        // (foo (foo) ((foo) foo) (((foo) foo) (foo) foo) ((((foo) foo) (foo) foo) ((foo) foo)
        //     (foo) foo) (((((foo) foo) (foo) foo) ((foo) foo) (foo) foo) (((foo) foo) (foo) foo)
        //     ((foo) foo) (foo) foo) ((((((foo) foo) (foo) foo) ((foo) foo) (foo) foo) (((foo) foo)
        //     (foo) foo) ((foo) foo) (foo) foo) ((((foo) foo) (foo) foo) ((foo) foo) (foo) foo)
        //     (((foo) foo) (foo) foo) ((foo) foo) (foo) foo))
        check("ff83666f6ffffe01fffe01fffe01fffe01fffe01fffe0180");
    }
}
