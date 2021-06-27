pub fn u64_to_bytes(n: u64) -> Vec<u8> {
    let mut buf = Vec::<u8>::new();
    buf.extend_from_slice(&n.to_be_bytes());
    if (buf[0] & 0x80) != 0 {
        buf.insert(0, 0);
    } else {
        while buf.len() > 1 && buf[0] == 0 && (buf[1] & 0x80) == 0 {
            buf.remove(0);
        }
    }
    buf
}

#[test]
fn test_u64_to_bytes() {
    assert_eq!(u64_to_bytes(0), &[0]);
    assert_eq!(u64_to_bytes(1), &[1]);
    assert_eq!(u64_to_bytes(0x7f), &[0x7f]);
    assert_eq!(u64_to_bytes(0x80), &[0, 0x80]);
    assert_eq!(u64_to_bytes(0xff), &[0, 0xff]);
    assert_eq!(
        u64_to_bytes(0xffffffffffffffff),
        &[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
    );
}
