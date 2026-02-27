# 2026 Serialization Format

## Magic Header

The 2026 serialization format is identified by a 3-byte magic prefix:

```
0xff 0x14 0x1a
```

Under the legacy CLVM deserializer, these bytes decode to:

- `0xff` — cons pair
- `0x14` — atom with value 20 (decimal)
- `0x1a` — atom with value 26 (decimal)

Producing the expression `(20 . 26)`.

### Detection

A deserializer determines the format by inspecting the first 3 bytes:

- If the blob is exactly `0xff 0x14 0x1a` (3 bytes, nothing more), it is the
  legacy-format value `(20 . 26)`.
- If the blob starts with `0xff 0x14 0x1a` and has additional bytes following,
  it is 2026-format. The payload begins at byte offset 3.

This works because `0xff 0x14 0x1a` is a complete, self-contained expression
under the legacy format. No valid legacy blob has trailing bytes after a
complete expression, so the presence of additional data is unambiguous.

### Backward Compatibility

When a 2026-format blob is handed to a legacy deserializer unaware of the new
format, one of two things happens:

1. The deserializer consumes the first 3 bytes, returns `(20 . 26)`, and
   ignores or discards the trailing bytes. The result is bland and obviously
   not a real program — a strong signal that something is up.
2. The deserializer rejects the blob due to unexpected trailing data. This is
   also acceptable: old tools cleanly refuse to process new-format data.

Either outcome is safe. No legitimate CLVM program is the bare value
`(20 . 26)`.

## Payload Format

After the 3-byte magic header, the payload consists of two sections:

1. **Atom table** — all unique atoms, grouped by length
2. **Instruction stream** — stack-based operations to reconstruct the tree

### Atom Table

The atom table begins with a varint encoding the number of distinct atom
lengths present.

For each length group (in ascending order of length):

- If there is **one** atom of that length: a positive varint encoding the
  length, followed by the atom's raw bytes.
- If there are **multiple** atoms of that length: a negative varint encoding
  the negated length, then a positive varint encoding the count, then the raw
  bytes of each atom concatenated (each is exactly `length` bytes).

Atoms are assigned indices starting from 0, in the order they appear in the
table (shortest atoms first).

### Instruction Stream

The instruction stream begins with a varint encoding the total number of
instructions.

Each instruction is a varint:

| Varint value | Meaning |
|---|---|
| Positive N | Push the atom at index N-1 onto the stack |
| Zero | Pop two items (right then left), cons them, push the pair |
| Negative -N | Push the already-constructed pair at index N-1 onto the stack |

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
bytes) form a two's-complement signed integer in big-endian order.
