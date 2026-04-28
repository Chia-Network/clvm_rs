# 2026 Serialization Format

## Magic Header

Every 2026-format blob begins with a 6-byte magic prefix:

```
0xfd 0xff 0x32 0x30 0x32 0x36
```

Rationale:

- `0xfd 0xff` drives legacy/backrefs decoders down an invalid atom-size path,
  causing immediate failure.
- `0x32 0x30 0x32 0x36` is ASCII `"2026"` for readable hexdumps.

### Detection

A deserializer determines the format by inspecting the first 6 bytes:

- If the blob starts with `0xfd 0xff 0x32 0x30 0x32 0x36`, it is 2026-format.
- Otherwise, parse with the legacy/backrefs path.

### Backward Compatibility

When a 2026-format blob is handed to a legacy deserializer unaware of the new
format, it should fail quickly due to the deliberately invalid size prefix.

## Payload Format

After the 6-byte magic header, the payload consists of two sections:

1. **Atom table** — all unique atoms (except nil), grouped by length
2. **Instruction stream** — stack-based operations to reconstruct the tree

### Atom Table

Nil (the empty atom) is **not** included in the atom table — it has a dedicated
opcode (`0`) in the instruction stream.

The atom table begins with a varint encoding the number of atom groups.

For each group (in stream order):

- If the group contains **one** atom: a positive varint encoding the atom's byte
  length, followed by the atom's raw bytes.
- If the group contains **multiple** atoms of the same length: a negative varint
  encoding the negated byte length, then a positive varint encoding the count,
  then the raw bytes of each atom concatenated (each is exactly `length` bytes).

Atoms are assigned indices starting from 0, in the order they appear in the
table.

The decoder accepts groups in any order. Multiple groups with the same byte
length are valid (they contribute separate atom indices). A serializer may
choose a specific ordering strategy (for example, sorting by frequency so
commonly-referenced atoms get smaller varint indices).

### Instruction Stream

The instruction stream begins with a varint encoding the total number of
instructions.

Each instruction is a varint:

| Varint value | Meaning                                                          |
| ------------ | ---------------------------------------------------------------- |
| `0`          | Push nil (the empty atom)                                        |
| `1`          | Pop two items (left was pushed first), cons them, push the pair  |
| `-1`         | Pop two items (right was pushed first), cons them, push the pair |
| `N >= 2`     | Push the atom at index N-2 onto the stack                        |
| `N <= -2`    | Push the already-constructed pair at index -N-2 onto the stack   |

Pairs are indexed in construction order (the first pair cons'd is index 0, the
second is index 1, etc.). A negative instruction references a pair that was
previously constructed during this same decode, enabling shared sub-trees
without re-encoding them.

After all instructions execute, the stack must contain exactly one item: the
root node.

### Varint Encoding

Signed integers are encoded with a variable-length prefix scheme:

```
0xxxxxxx                          →  7-bit value, range [-64, 63]
10xxxxxx xxxxxxxx                 → 14-bit value, range [-8192, 8191]
110xxxxx xxxxxxxx xxxxxxxx        → 21-bit value, range [-1048576, 1048575]
...
```

The number of leading `1` bits determines how many additional bytes follow. A
`0` separator bit follows the leading `1`s. The remaining bits (across all
bytes) form a two's-complement signed integer in big-endian order. This scales
to wider integers without changing the encoding rules.

A prefix of 8 leading `1` bits (`0xFF`) is invalid.
