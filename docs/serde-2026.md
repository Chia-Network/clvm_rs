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

The magic prefix allows helper APIs to distinguish 2026-format blobs from
legacy/backref blobs:

- If the blob starts with `0xfd 0xff 0x32 0x30 0x32 0x36`, it is 2026-format.
- Otherwise, parse with the legacy/backrefs path.

Consensus callers do not need to rely on auto-detection. They can select the
expected format from fork height or consensus flags and call the corresponding
deserializer directly.

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

Atom lengths must be non-zero because nil is excluded from the atom table.
Deserializers enforce a configurable maximum atom length (default: 1 MiB) and a
maximum input byte budget (default: 10 MiB). Separate atom-group, atom-count,
instruction-count, stack-size, and pair-count limits are not needed for DoS
protection: every declared item must consume at least one input byte before it
can produce parser work or allocate a CLVM node. The input byte budget therefore
bounds all of those quantities.

Atoms are assigned indices starting from 0, in the order they appear in the
table.

The decoder accepts groups in any order. Multiple groups with the same byte
length are valid (they contribute separate atom indices). A serializer may
choose a specific ordering strategy (for example, sorting by frequency so
commonly-referenced atoms land in lower index ranges whose varint encodings are
shorter).

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

The current serializer emits left-first cons instructions (`1`). Decoders accept
right-first cons instructions (`-1`) so future serializers can choose different
pair visit orders without changing the wire format.

After all instructions execute, the stack must contain exactly one item: the
root node.

### Varint Encoding

Signed integers are encoded with a variable-length prefix scheme:

```
0xxxxxxx                          →  7-bit value, range [-64, 63]
10xxxxxx xxxxxxxx                 → 14-bit value, range [-8192, 8191]
110xxxxx xxxxxxxx xxxxxxxx        → 21-bit value, range [-1048576, 1048575]
1110xxxx xxxxxxxx xxxxxxxx xxxxxxxx
                                   → 28-bit value
11110xxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx
                                   → 35-bit value
111110xx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx
                                   → 42-bit value
1111110x xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx
                                   → 49-bit value
11111110 xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx
                                   → 56-bit value
```

The number of leading `1` bits determines how many additional bytes follow,
similar to UTF-8 prefix-length coding. A `0` separator bit follows the leading
`1`s. The remaining bits (across all bytes) form a two's-complement signed
integer in big-endian order.

A prefix of 8 leading `1` bits (`0xFF`) is invalid.

The deserializer has a `strict` mode that rejects overlong varint encodings. In
strict mode, every varint must use the shortest encoding that can represent its
value. Lenient mode accepts overlong encodings for tooling/backward-compatible
parsing.

## Size Bound

For the current instruction-stream format, the analysis in
`generator-identity-hf-analysis/docs/SERDE2026_UPPER_BOUND.md` proves:

```
serde_2026_bytes <= atom_bytes + 2 * unique_atoms + 3 * unique_pairs + 5
```

assuming all atom lengths fit in a 4-byte varint (`length <= 2^27 - 1`). This
condition is far weaker than the default 1 MiB atom limit. Because the hard fork
cost formula charges this same size component, consensus callers can derive
their accepted serde_2026 byte budget from the 11B block cost limit instead of
choosing an arbitrary message-size cap.
