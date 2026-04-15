# 2026 Serialization Format

## Magic Header

The 2026 serialization format is identified by a 6-byte magic prefix:

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

1. **Atom table** — all unique atoms, grouped by length
2. **Instruction stream** — stack-based operations to reconstruct the tree

### Atom Table

The atom table begins with a varint encoding the number of distinct atom
lengths present.

For each length group (in stream order):

- If there is **one** atom of that length: a positive varint encoding the
  length, followed by the atom's raw bytes.
- If there are **multiple** atoms of that length: a negative varint encoding
  the negated length, then a positive varint encoding the count, then the raw
  bytes of each atom concatenated (each is exactly `length` bytes).

Atoms are assigned indices starting from 0, in the order they appear in the
table.

The decoder accepts length groups in any order, and repeated length groups are
valid. A serializer may still choose a specific ordering strategy (for example,
sorting by length) as an implementation optimization.

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

The default serializer always uses opcode `1` (left-first cons). The
pair-optimized serializer uses both `1` and `-1` to steer traversal order,
reducing the number of pair back-references needed.

Pairs are indexed in construction order (the first pair cons'd is index 0, the
second is index 1, etc.). A negative instruction references a pair that was
previously constructed during this same decode, enabling shared sub-trees
without re-encoding them.

After all instructions execute, the stack must contain exactly one item: the
root node.

### Varint Encoding

Signed integers are encoded with a variable-length prefix scheme.
The patterns below are illustrative examples, not an upper bound:

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
