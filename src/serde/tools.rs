use std::io;
use std::io::{Cursor, Read, Seek, SeekFrom};

use super::errors::bad_encoding;
use super::parse_atom::decode_size;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

pub fn serialized_length_from_bytes(b: &[u8]) -> io::Result<u64> {
    let mut f = Cursor::new(b);
    let mut ops_counter = 1;
    let mut b = [0; 1];
    while ops_counter > 0 {
        ops_counter -= 1;
        f.read_exact(&mut b)?;
        if b[0] == CONS_BOX_MARKER {
            // we expect to parse two more items from the strem
            // the left and right sub tree
            ops_counter += 2;
        } else if b[0] == BACK_REFERENCE {
            // This is a back-ref. We don't actually need to resolve it, just
            // parse the path and move on
            let mut first_byte = [0; 1];
            f.read_exact(&mut first_byte)?;
            if first_byte[0] > MAX_SINGLE_BYTE {
                let path_size = decode_size(&mut f, first_byte[0])?;
                f.seek(SeekFrom::Current(path_size as i64))?;
                if (f.get_ref().len() as u64) < f.position() {
                    return Err(bad_encoding());
                }
            }
        } else if b[0] == 0x80 || b[0] <= MAX_SINGLE_BYTE {
            // This one byte we just read was the whole atom.
            // or the special case of NIL
        } else {
            let blob_size = decode_size(&mut f, b[0])?;
            f.seek(SeekFrom::Current(blob_size as i64))?;
            if (f.get_ref().len() as u64) < f.position() {
                return Err(bad_encoding());
            }
        }
    }
    Ok(f.position())
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

#[repr(u8)]
enum ParseOp {
    SExp,
    Cons,
}

// computes the tree-hash of a CLVM structure in serialized form
pub fn tree_hash_from_stream(f: &mut Cursor<&[u8]>) -> io::Result<[u8; 32]> {
    let mut values: Vec<[u8; 32]> = Vec::new();
    let mut ops = vec![ParseOp::SExp];

    let mut b = [0; 1];
    while let Some(op) = ops.pop() {
        match op {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if b[0] == 0x80 {
                    values.push(hash_atom(&[]));
                } else if b[0] <= MAX_SINGLE_BYTE {
                    values.push(hash_atom(&b));
                } else {
                    let blob_size = decode_size(f, b[0])?;
                    let blob = &f.get_ref()[f.position() as usize..];
                    if (blob.len() as u64) < blob_size {
                        return Err(bad_encoding());
                    }
                    f.set_position(f.position() + blob_size);
                    values.push(hash_atom(&blob[..blob_size as usize]));
                }
            }
            ParseOp::Cons => {
                // cons
                let v2 = values.pop();
                let v1 = values.pop();
                values.push(hash_pair(&v1.unwrap(), &v2.unwrap()));
            }
        }
    }
    Ok(values.pop().unwrap())
}

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

#[cfg(test)]
use hex::FromHex;

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
        Vec::from_hex("123190dddde51acfc61f48429a879a7b905d1726a52991f7d63349863d06b1b6").unwrap()
    );
}

#[test]
fn test_tree_hash_tree() {
    // this is the tree ((1, 2), (3, 4))
    let buf = Vec::from_hex("ffff0102ff0304").unwrap();
    let mut cursor = Cursor::<&[u8]>::new(&buf);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
        Vec::from_hex("2824018d148bc6aed0847e2c86aaa8a5407b916169f15b12cea31fa932fc4c8d").unwrap()
    );
}

#[test]
fn test_tree_hash_tree_large_atom() {
    // this is the tree ((1, 2), (3, b"foobar"))
    let buf = Vec::from_hex("ffff0102ff0386666f6f626172").unwrap();
    let mut cursor = Cursor::<&[u8]>::new(&buf);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
        Vec::from_hex("b28d5b401bd02b65b7ed93de8e916cfc488738323e568bcca7e032c3a97a12e4").unwrap()
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
        serialized_length_from_bytes(&[0x8f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        16
    );
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::serde::node_from_bytes_backrefs;
    use crate::Allocator;
    use rstest::rstest;

    #[rstest]
    // ("foobar" "foobar")
    #[case("ff86666f6f626172ff86666f6f62617280")]
    // ("foobar" "foobar")
    #[case("ff86666f6f626172fe01")]
    // ((1 2 3 4) 1 2 3 4)
    #[case("ffff01ff02ff03ff0480ff01ff02ff03ff0480")]
    // ((1 2 3 4) 1 2 3 4)
    #[case("ffff01ff02ff03ff0480fe02")]
    // `(((((a_very_long_repeated_string . 1) .  (2 . 3)) . ((4 . 5) .  (6 . 7))) . (8 . 9)) 10 a_very_long_repeated_string)`
    #[case(
        "ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff04\
05ff0607ff0809ff0aff9b615f766572795f6c6f6e675f72657065617465645f737472696e6780"
    )]
    #[case("ffffffffff9b615f766572795f6c6f6e675f72657065617465645f737472696e6701ff0203ffff0405ff0607ff0809ff0afffe4180")]
    #[case(
        "ff01ffffffa022cf3c17be4e0e0e0b2e2a3f6dd1ee955528f737f0cb724247bc2e4a776cb989ff\
ff02ffff01ff02ffff01ff02ffff03ffff18ff2fffff010180ffff01ff02ff36ffff04ff02ffff\
04ff05ffff04ff17ffff04ffff02ff26ffff04ff02ffff04ff0bff80808080ffff04ff2fffff04\
ff0bffff04ff5fff808080808080808080ffff01ff088080fe81ffffff04ffff01ffffffff4602\
ff3304ffff0101ff02ffff02ffff03ff05ffff01ff02ff5cffff04ff02ffff04ff0dffff04ffff\
0bff2cffff0bff24ff3880ffff0bff2cffff0bff2cffff0bff24ff3480ff0980ffff0bff2cff0b\
ffff0bff24ff8080808080ff8080808080ffff010b80ff0180ff02ffff03ff0bffff01ff02ff32\
ffff04ff02ffff04ff05ffff04ff0bffff04ff17ffff04ffff02ff2affff04ff02ffff04ffff02\
ffff03ffff09ff23ff2880ffff0181b3ff8080ff0180fe7a8080ff80808080808080ffff01ff02\
ffff03ff17ff80fe837b7fffff018080fe3bffffffff0bffff0bff17ffff02ff3affff04ff02ff\
ff04ff09ffff04ff2fffff04ffff02ff26ffff04ff02ffff04ff05ff80808080ff808080808080\
ff5f80ff0bff81bf80ff02ffff03ffff20ffff22ff4fff178080ffff01ff02ff7effff04ff02ff\
ff04ff6fffff04ffff04ffff02ffff03ff4fffff01ff04ff23ffff04ffff02ff3affff04ff02ff\
ff04ff09ffff04ff53fe861db6d7ffffff808080ffff04ff81b3ff80808080ffff011380ff0180\
ffff02ff7cffff04ff02ffff04ff05ffff04ff1bffff04ffff21ff4fff1780ff80808080808080\
ff8080808080fe82f6ffff0180ffff04ffff09ffff18ff05fe8301d6fffe0effff09ff05ffff01\
818f8080ff0bff2cffff0bff24ff3080ffff0bff2cffff0bff2cffff0bff24ff3480ff0580ffff\
0bff2cffff02ff5cffff04ff02ffff04ff07ffff04ffff0bff24ff2480ff8080808080fe861eee\
b6ed77ff8080ffffff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff26ffff04ff02ff\
ff04ff09ff80808080ffff02ff26ffff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101\
fe6f80ff0180ff02ff5effff04ff02ffff04ff05ffff04ff0bffff04ffff02ff3affff04ff02ff\
ff04ff09ffff04ff17fe853b6da3ffff808080ffff04ff17ffff04ff2fffff04ff5fffff04ff81\
bfff80808080808080808080ffff04ffff04ff20ffff04ff17ff808080ffff02ff7cffff04ff02\
ffff04ff05ffff04ffff02ff82017fffff04ffff04ffff04ff17ff2f80ffff04ffff04ff5fff81\
bf80ffff04ff0bff05808080ff8202ff8080ffff01ff80808080808080ffff02ff2effff04ff02\
ffff04ff05ffff04ff0bffff04ffff02ffff03ff3bffff01ff02ff22ffff04ff02ffff04ff05ff\
ff04ff17ffff04ff13ffff04ff2bffff04ff5bffff04ff5fff808080808080808080ffff01ff02\
ffff03ffff09ff15ffff0bff13ff1dff2b8080ffff01ff0bff15ff17ff5f80fe841ecfffffff01\
8080ff0180ffff04ff17ffff04ff2fffff04ff5fffff04ff81bfffff04ff82017fff8080808080\
808080808080ff02ffff03ff05ffff011bfe8307aeffff0180fe7b80ffff04ffff01ffa024e044\
101e57b3d8c908b8a38ad57848afd29d3eecc439dba45f4412df4954fdffa0f08eda9271f9dd1c\
00d9789ba0f3e4f547d66c8ad6f75edbb587b8c68d8ef5f1a0eff07522495060c066f66f32acc2\
a77e3a3e737aca8baea4d1a64ea4cdc13da9ffff04ffff01ff02ffff01ff02ffff01ff02ffff03\
ff8202ffffff01ff02ff16ffff04ff02ffff04ff05ffff04ff8204bfffff04ff8206bfffff04ff\
82017fffff04ffff0bffff19ff2fffff18ffff019100ffffffffffffffffffffffffffffffffff\
8202ff8080ff0bfffe2f80ff8080808080808080ffff01ff04ffff04ff08ffff04ff17ffff04ff\
ff02ff1effff04ff02fe8876db6db7d77fffff80ff80808080ffff04ffff04ff1cffff04ff5fff\
ff04ff8206bfff80808080ff80808080ff0180ffff04ffff01ffff32ff3d33ff3effff04ffff04\
ff1cffff04ff0bfe8503af5dffff80ffff04ffff04ff1cffff04ff05ffff04ff2fff80808080ff\
ff04ffff04ff0afe87076db6edb7ffffffff04ffff04ff14ffff04ffff0bff5fffff012480ff80\
8080fe76808080ff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff1efe861dda75dfff\
ffffff02ff1efe8677b4ebbfffff80fe8507a75dffffff0180fe7b80ffff04ffff01a02f2c9ba1\
b2315d413a92b5f034fa03282ccba1767fd9ae7b14d942b969ed5d57ffff04ffff01a0c4a8fb8a\
651c5e8636c6dd67ed8c8f7a70f516f41ab73abd14dd79c7582f079affff04ffff01b08116639d\
853ecd6109277a9d83d3acc7e53a18d3524262ec9b99df923d22a390cbf0f632bced556dd9886b\
bf53f444b6ffff04ffff01a0ccd5bb71183532bff220ba46c268991a0000000000000000000000\
0000000000ffff04ffff01a022c0df3c66541eb57c226e12742a9f7182ceb0318cdaf5a84facb6\
c80bfaac1aff01808080808080fe81ff8080ff01ffffffa01daef44c653c413eba01d89790edfb\
613cb020ed0979d8447d6ed398327d0958ffa0976299e4fbb74e8732ae1ea7092ab617af435218\
f20912f99689d8fc69d12318fe3fff01ffffffff70c07101022f2c9ba1b2315d413a92b5f034fa\
03282ccba1767fd9ae7b14d942b969ed5d578116639d853ecd6109277a9d83d3acc7e53a18d352\
4262ec9b99df923d22a390cbf0f632bced556dd9886bbf53f444b6010000001668747470733a2f\
2f6575312e706f6f6c2e73706163650000004080ff80808080ffffa0e8058c610f8f50e5e603ad\
136f58ff3b1f0120a76bb4985553db8bda88738ca9ffff02fffe81abffff04ffff01fffe82ab5f\
ffa0df84418c7ba1b1a847fa23bd1101fcee22b2bd81c69d10e6d4b280f09eba2c70fe8307ad7f\
ffff04ffff01ff02fffe835adaffffff04fffe840aeb6bffffff04ffff01a09c117e3fba164415\
fb868361a5c6d9c8a4551c71e5a359807cb27810b80f23bdffff04ffff01b08a505367d099210c\
24f1be8945a83de7db6d9396a9742c8f609aebf7ba56bbaacb038819b122741211bbc9227c9035\
73ffff04fffe86056dbadaffffffff04ffff01a0a78f9cbed5f7e1b529aa32088dcc3c53ac1df5\
7bd0c8f4773f4c0ddae048a8edff01808080808080ff01808080ff01ffffffa0245c7e4ad426a2\
a6b6eb5622286d2fd8178fca04cdb9a23aeb8da08ae6d6e6c0ffa0c79f2213f98b1ac6bfa141b6\
724138223236e89ae456e6c6727f8709e4b28ed6fe7fff01ffffffff70c07101022f2c9ba1b231\
5d413a92b5f034fa03282ccba1767fd9ae7b14d942b969ed5d578a505367d099210c24f1be8945\
a83de7db6d9396a9742c8f609aebf7ba56bbaacb038819b122741211bbc9227c90357301000000\
1668747470733a2f2f6e61312e706f6f6c2e73706163650000004080ff80808080ffffa0bc334d\
2f6b030790debd7e4a998a38d0628b2b853fa7895db16ce14da37c441dffff02fffe81abffff04\
ffff01fffe82ab5fffa01a3272c823c1175f83016303bfd33823687df39736e2b7eaf9057f0067\
1d9f97fe8307ad7fffff04ffff01ff02fffe835adaffffff04ffff01a09d9c5296f00b89c2271a\
b4a00f249ab3a0106d8d73dd02242f3ea6357b4cde04ffff04ffff01a0e691c76d0108a7a7a445\
91b3784ecf4099bf5fb057173a0bb2f79fdfa2ed9fceffff04ffff01b0ab6824901d856c5a8c16\
64c990d1ef94a19ed7b4ab28a6b8e064f1a11e07f5e75bdb6ff8242f517534df36ae03c81da0ff\
ff04fffe86056dbadaffffffff04ffff01a0ab43e98b62a2dc33caf0b16fb5a3c037e3303fc8e1\
77ddf437eef233c87d32f2ff01808080808080ff01808080ff01ffffffa00afb9bf9c6a13d380c\
9645ae07e466bf71a171ae6fe35bc3a7ac5fb5df53a937ffa0cec914e2de9e524237e7a76f3d48\
3a9cf1674be213d4c5bf51fe28b6dab92898ff0180ff01ffffffff70c07001029d9c5296f00b89\
c2271ab4a00f249ab3a0106d8d73dd02242f3ea6357b4cde04ab6824901d856c5a8c1664c990d1\
ef94a19ed7b4ab28a6b8e064f1a11e07f5e75bdb6ff8242f517534df36ae03c81da00100000015\
68747470733a2f2f636869612e64706f6f6c2e63630000002080ff80808080ffffa07016fd25c1\
4831bfe48a08cd3cd9eeb6d416436087252e1061e62cdc95cc892affff02fffe81abffff04ffff\
01fffe82ab5fffa0cb75e5c90f2ab4bdf1db8a420e56e9edb261b919b73a6f8756453c07d73d43\
0ffe8307ad7fffff04ffff01ff02ffff01ff02ffff01ff02ffff03ff82017fffff01ff04ffff04\
ff1cfe8676db6edb7fffffff04ffff04ff12ffff04ff8205fffe8803b5ddb6b6bfffff80ffff04\
ffff04ff08ffff04ff17ffff04ffff02ff1effff04ff02ffff04ffff04ff8205ffffff04ff8202\
ffff808080fe768080ff80808080ff80808080ffff01ff02ff16ffff04ff02ffff04ff05ffff04\
ff8204bfffff04ff8206bfffff04ff8202ffffff04ffff0bffff19ff2fffff18fffe8b15ab6db7\
6db5b5ffffffffff8205ff8080ff0bfffe2f80ff808080808080808080ff0180ffff04ffff01ff\
ff32ff3d52ffff333effff04ffff04ff12fe8675d76b6bffffffff04ffff04ff12fe870eb75dad\
afffffffff04ffff04ff1afe83edb7fffe873b6ebb5b5fffff8080fe8601f5dadafffffe7b80ff\
ff04fffe840aeb6bffffff04ffff01a0a219765e3616e24fb86a7ddace966d0aacfcdb8d8b6823\
e9760e6b4a0469e07affff04ffff01b0afdbc8d2811665196a20931b06ffe981a2ec64aebd2d91\
7478bf8441d77cb2b62f96194277d91983c5ca9edf0a17fdccffff04fffe86056dbadaffffffff\
04ffff0120ff01808080808080ff01808080ff01ffffffa0b3957791c7e84aa27e759b217ef972\
85c3c260b0410f230679a00a346ee695b4ffa03732f9605848b5ee1fe700dd36aab3aa23110d35\
c0e5917676d042883330c39aff0180ff01ffff01ffffff70c0570101016127e457a90eb1229665\
8006a7928d5acffbe3c707b705177efe504d1beae0afdbc8d2811665196a20931b06ffe981a2ec\
64aebd2d917478bf8441d77cb2b62f96194277d91983c5ca9edf0a17fdcc000000000080ffa062\
e47157eea5430b839a8fcd8390ce3c162b61acd19f94eeb97db81d7622a52e808080ffffa0b63a\
7ce25b1050bfd97a9e4e67fcf42ca6545eaf392a13c67307ee3614e59bc2ffff02fffe81abffff\
04ffff01fffe82ab5fffa05a7cdf809298a6c314bfd6ec63c6761b396fca802c0067d87de23644\
d17a8bb5fe8307ad7fffff04ffff01ff02fffe835adaffffff04fffe840aeb6bffffff04ffff01\
a06dd75452b3a13128591f83a68263fb0dc9bedfb519af3af01933782ae65dca1bffff04ffff01\
b08e2ab4bd0f4b65f0e6e1cc1f54fd3a953a36afc98ec25a741958bf1f19d0a416f2b39b89d4bd\
9870f11d6bf09030780efe8576dd6d7fff808080ff01808080ff01ffffffa0f447a4cda2bd4e2c\
14398f96d1402e8ed0c35302b0c57f8219861d9433ba150cffa0eb4e5fc93add634ac692d2c28b\
0eed8dcdca399639cd7986dc6879c7a872e5f4ff0180ff01ffff01ffffff70c07701030213db9c\
b540a52c39071ef70acdf81796303189061b5aa0ee8098a05d5ceff58e2ab4bd0f4b65f0e6e1cc\
1f54fd3a953a36afc98ec25a741958bf1f19d0a416f2b39b89d4bd9870f11d6bf09030780e0100\
00001c68747470733a2f2f7669702e746565706f6f6c2e636f6d3a393434330000002080ffa0a6\
322ec0a3d445a051349cf8289660b2d9686608484ecb42c135fd5bd4ea4b11808080ffffa077c5\
2e138369b3a545a59f3daf2904197f350010676506eead7f5875f511f063ffff02fffe81abffff\
04ffff01fffe82ab5fffa00485de73367a4b49eb34c7f80df3c9b70c739411d40632428e42495d\
a7c0ba91fe8307ad7fffff04ffff01ff02fffe835adaffffff04fffe840aeb6bffffff04ffff01\
a0671ce6fd24697788441f93773e5905c1f1153ba7f5d3fbb6a4e2855a05b42731ffff04ffff01\
b0a5383a3b9a6c1a94a85e4f982e1fa3af2c99087e5f6df8b887d30c109f71043671683a1ae985\
d7d874fbe07dfa6d88b7fe8576dd6d7fff808080ff01808080ff01ffffffa01dc8c9c3356a6eef\
ee37744901b3f730c710052bd38deac264ecea45460faffeffa031460205316513dc6b11d09934\
62776f0994223cac2f215bf8afbeae5a0cce3fff0180ff01ffff01ffffff70c0730103e3b9adc4\
eb09d18d83bd56482aee566820f5afdce595b3ed095eb36c5cef9301a5383a3b9a6c1a94a85e4f\
982e1fa3af2c99087e5f6df8b887d30c109f71043671683a1ae985d7d874fbe07dfa6d88b70100\
00001868747470733a2f2f6368696170702e68706f6f6c2e636f6d0000004080ffa0fc0b1e9409\
ae5c3c40c50832a7aecc0b3ba4646568a00c01289c45e1f03b2b488080808080"
    )]
    fn serialized_length_with_backrefs(#[case] serialization_as_hex: &str) {
        let buf = Vec::from_hex(serialization_as_hex).unwrap();
        let len = serialized_length_from_bytes(&buf).expect("serialized_length_from_bytes");

        // make sure the serialization is valid
        let mut allocator = Allocator::new();
        assert!(node_from_bytes_backrefs(&mut allocator, &buf).is_ok());

        assert_eq!(len, buf.len() as u64);
    }
}
