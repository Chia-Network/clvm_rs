use std::io;
use std::io::ErrorKind;

/// all atoms serialize their contents verbatim. All expect those one-byte atoms
/// from 0x00-0x7f also have a prefix encoding their length. This function
/// writes the correct prefix for an atom of size `size` whose first byte is `atom_0`.
/// If the atom is of size 0, use any placeholder first byte, as it's ignored anyway.

fn write_atom_encoding_prefix_with_size<W: io::Write>(
    f: &mut W,
    atom_0: u8,
    size: u64,
) -> io::Result<()> {
    if size == 0 {
        f.write_all(&[0x80])
    } else if size == 1 && atom_0 < 0x80 {
        Ok(())
    } else if size < 0x40 {
        f.write_all(&[0x80 | (size as u8)])
    } else if size < 0x2000 {
        f.write_all(&[0xc0 | (size >> 8) as u8, size as u8])
    } else if size < 0x10_0000 {
        f.write_all(&[
            (0xe0 | (size >> 16)) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])
    } else if size < 0x800_0000 {
        f.write_all(&[
            (0xf0 | (size >> 24)) as u8,
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])
    } else if size < 0x4_0000_0000 {
        f.write_all(&[
            (0xf8 | (size >> 32)) as u8,
            ((size >> 24) & 0xff) as u8,
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])
    } else {
        Err(io::Error::new(ErrorKind::InvalidData, "atom too big"))
    }
}

/// serialize an atom
pub fn write_atom<W: io::Write>(f: &mut W, atom: &[u8]) -> io::Result<()> {
    let u8_0 = if !atom.is_empty() { atom[0] } else { 0 };
    write_atom_encoding_prefix_with_size(f, u8_0, atom.len() as u64)?;
    f.write_all(atom)
}

#[test]
fn test_write_atom_encoding_prefix_with_size() {
    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0, 0).is_ok());
    assert_eq!(buf, vec![0x80]);

    for v in 0..0x7f {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, v, 1).is_ok());
        assert_eq!(buf, vec![]);
    }

    for v in 0x80..0xff {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, v, 1).is_ok());
        assert_eq!(buf, vec![0x81]);
    }

    for size in 0x1_u8..0x3f_u8 {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, size as u64).is_ok());
        assert_eq!(buf, vec![0x80 + size]);
    }

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0b111111).is_ok());
    assert_eq!(buf, vec![0b10111111]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0b1000000).is_ok());
    assert_eq!(buf, vec![0b11000000, 0b1000000]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0xfffff).is_ok());
    assert_eq!(buf, vec![0b11101111, 0xff, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0xffffff).is_ok());
    assert_eq!(buf, vec![0b11110000, 0xff, 0xff, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0xffffffff).is_ok());
    assert_eq!(buf, vec![0b11111000, 0xff, 0xff, 0xff, 0xff]);

    // this is the largest possible atom size
    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0x3ffffffff).is_ok());
    assert_eq!(buf, vec![0b11111011, 0xff, 0xff, 0xff, 0xff]);

    // this is too large
    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0x400000000).is_err());

    for (size, expected_prefix) in [
        (0x1, vec![0x81]),
        (0x2, vec![0x82]),
        (0x3f, vec![0xbf]),
        (0x40, vec![0xc0, 0x40]),
        (0x1fff, vec![0xdf, 0xff]),
        (0x2000, vec![0xe0, 0x20, 0x00]),
        (0xf_ffff, vec![0xef, 0xff, 0xff]),
        (0x10_0000, vec![0xf0, 0x10, 0x00, 0x00]),
        (0x7ff_ffff, vec![0xf7, 0xff, 0xff, 0xff]),
        (0x800_0000, vec![0xf8, 0x08, 0x00, 0x00, 0x00]),
        (0x3_ffff_ffff, vec![0xfb, 0xff, 0xff, 0xff, 0xff]),
    ] {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, size).is_ok());
        assert_eq!(buf, expected_prefix);
    }
}

#[test]
fn test_write_atom() {
    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &[]).is_ok());
    assert_eq!(buf, vec![0b10000000]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &[0x00]).is_ok());
    assert_eq!(buf, vec![0b00000000]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &[0x7f]).is_ok());
    assert_eq!(buf, vec![0x7f]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &[0x80]).is_ok());
    assert_eq!(buf, vec![0x81, 0x80]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &[0xff]).is_ok());
    assert_eq!(buf, vec![0x81, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &[0xaa, 0xbb]).is_ok());
    assert_eq!(buf, vec![0x82, 0xaa, 0xbb]);

    for (size, mut expected_prefix) in [
        (0x1, vec![0x81]),
        (0x2, vec![0x82]),
        (0x3f, vec![0xbf]),
        (0x40, vec![0xc0, 0x40]),
        (0x1fff, vec![0xdf, 0xff]),
        (0x2000, vec![0xe0, 0x20, 0x00]),
        (0xf_ffff, vec![0xef, 0xff, 0xff]),
        (0x10_0000, vec![0xf0, 0x10, 0x00, 0x00]),
        (0x7ff_ffff, vec![0xf7, 0xff, 0xff, 0xff]),
        (0x800_0000, vec![0xf8, 0x08, 0x00, 0x00, 0x00]),
        // the next one represents 17 GB of memory, which it then has to serialize
        // so let's not do it until some time in the future when all machines have
        // 64 GB of memory
        // (0x3_ffff_ffff, vec![0xfb, 0xff, 0xff, 0xff, 0xff]),
    ] {
        let mut buf = Vec::<u8>::new();
        let atom = vec![0xaa; size];
        assert!(write_atom(&mut buf, &atom).is_ok());
        expected_prefix.extend(atom);
        assert_eq!(buf, expected_prefix);
    }
}
