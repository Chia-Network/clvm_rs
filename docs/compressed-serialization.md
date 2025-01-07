# Compressed Serialization

With the Chia hard fork at height 5'496'000, CLVM can be serialized in a more
space efficient form, referring back to previous sub-trees instead of
duplicating them. These references are referred to as "back references".

## Format

The original serialization format had 3 tokens.

- `0xff` - a pair, followed by the left and right sub-trees.
- Atom - values, in the form of an array of bytes. For more details, see [CLVM
  serialization](https://chialisp.com/clvm/#serialization).

A back reference is introduced by `0xfe` followed by an atom. The atom refers
back to an already decoded sub tree. The bits are interpreted just like an
environment lookup in CLVM. The bits are inspected one at a time, from least
significant to most significant bits, in big-endian order.

## Paths

```
            +----------+----------+----------+----------+
byte index: |  byte 0  |  byte 1  |  byte 2  |  byte 3  |
            +----------+----------+----------+----------+
 bit index: | 76543210 | 76543210 | 76543210 | 76543210 |
            +----------+----------+----------+----------+

bit traversal direction:                          <- x
```

A `0` bit means follow the left sub-tree while a `1` bit means follow the right
sub-tree. The last 1-bit is the terminator, and means we should pick the node at
the current location in the tree.

e.g. The reference `0b1011` means:

- right
- right
- left
- (terminator bit)

It follows the path below:

```
                 [*]
                /   \
               /     \
              /       \ 1
             /         \
            /           \
           /             \
         [ ]             [*]
        /   \           /   \ 1
       /     \         /     \
     [ ]     [ ]     [ ]     [*]
     / \     / \     /  \  0 / \
   [ ] [ ] [ ] [ ] [ ] [ ] [*] [ ]
```

How environment lookups work is also described in the
[chialisp documentation](https://chialisp.com/clvm/#environment).

## Parsing

Back references refer into the "parse stack". This is a CLVM tree that's updated
as we parse, so what a back reference refers to changes as we parse the
serialized CLVM tree. To understand what the parse stack is, we first need to
look at how CLVM is parsed.

The parser has a stack of _operations_ and a stack of the parsed results (the
parse stack).

There are 2 operations that can be pushed onto the operations stack:

- `Cons` - Construct a pair (cons box)
- `Traverse` - parse a sub-tree

As outlined in the [Format](#Format) section, there are two tokens we can
encounter when parsing; an atom or a pair (followed by the left- and right
sub-trees).

We keep popping operations off of the op-stack until it's empty. We take the
following actions depending on the operation:

- `Traverse`, inspect the next byte of the input stream. If it's a pair (`0xff`)
  we push `Cons`, `Traverse`, `Traverse` onto the operations stack. If it's an
  atom, parse the atom and push it into the parse stack.

- `Cons`, pop two nodes from the parse stack, create a new pair with those nodes
  as the left and right side. Push the resulting pair onto the stack.

### Example

To parse the tokens: `0xff` `1` `0xff` `2` `foobar`, the two stacks end up like
this while parsing. The stacks grow to the right in this illustration.

| step              | op-stack                       | parse-stack              |
| ----------------- | ------------------------------ | ------------------------ |
| 1, initial state  | Traverse                       |                          |
| 2, parse `0xff`   | Cons, Traverse, Traverse       |                          |
| 3, parse `1`      | Cons, Traverse                 | `1`                      |
| 4, parse `0xff`   | Cons, Cons, Traverse, Traverse | `1`                      |
| 5, parse `2`      | Cons, Cons, Traverse           | `1`, `2`                 |
| 6, parse `foobar` | Cons, Cons                     | `1`, `2`, `foobar`       |
| 7, pop2 and cons  | Cons                           | `1`, (`2` . `foobar`)    |
| 8, pop2 and cons  |                                | (`1` . (`2` . `foobar`)) |

## Parse stack

When a back-reference token (`0xfe`) is encountered, the parse stack in that
current state is used as the environment for the back-reference path to look up
what node to place at this position in the resulting tree.

The parse stack is itself a LISP list of items. The top of the stack is the head
of the list.

e.g.

The stack `1`, `2`, `3`, would have the following LISP structure:

```
(`1` . (`2` . (`3` . NIL)))
```

A back reference to `3` would be: `0b1100` (right, left).

### Example back-reference

Consider the following LISP structure: ((`1` . `2`) . (`1` . `2`))  
It can be serialized as `0xff` `0xff` `1` `2` `0xfe` `0b10`

The parsing steps would be as follows:

| step                | op-stack                                 | parse-stack                 |
| ------------------- | ---------------------------------------- | --------------------------- |
| 1, initial state    | Traverse                                 |                             |
| 2, parse `0xff`     | Cons, Traverse, Traverse                 |                             |
| 3, parse `0xff`     | Cons, Traverse, Cons, Traverse, Traverse |                             |
| 4, parse `1`        | Cons, Traverse, Cons, Traverse           | `1`                         |
| 5, parse `2`        | Cons, Traverse, Cons                     | `1`, `2`                    |
| 6, pop2 and cons    | Cons, Traverse                           | (`1` . `2`)                 |
| 7, parse `0xfe` `2` | Cons                                     | (`1` . `2`), (`1` . `2`)    |
| 8, pop2 and cons    |                                          | ((`1` . `2`) . (`1` . `2`)) |

### Referencing the stack itself

Back references aren't limited to just referencing items in the stack, but can
reference any node in the stack. For example, consider parsing the following
structure:

`0xff` `foobar` `0xff` `foobar` NIL

| step              | op-stack                 | parse-stack |
| ----------------- | ------------------------ | ----------- |
| 1, initial state  | Traverse                 |             |
| 2, parse `0xff`   | Cons, Traverse, Traverse |             |
| 3, parse `foobar` | Cons, Traverse           | `foobar`    |

At this point, rather than parsing the next `0xff` pair, we could have a back
reference (`0xfe`) with a path pointing to the root of the parse stack. In LISP
form, the parse stack will be (`foobar` . NIL) - a list with one item. The rest
of the CLVM tree is just the second `foobar` followed by the list terminator. It
can be replaced with the parse stack itself. i.e. We can use a back-reference of
`1`. We then get the NIL and the cons box "for free". It's implied by the parse
stack.

In this scenario, the rest of the parsing steps are:

| step                | op-stack | parse-stack                   |
| ------------------- | -------- | ----------------------------- |
| 4, parse `0xfe` `1` | Cons     | `foobar`, (`foobar` . NIL)    |
| 5, pop2 and cons    |          | (`foobar` . (`foobar` . NIL)) |

In practice, however, this rarely happens.

## Generating back references

When serializing with compression, we need to assign a tree-hash and an
(uncompressed) serialized length to every node. When deciding whether to output
the sub-tree itself or a back-reference, we need to know whether we have already
serialized an identical sub tree. If we have, we then have to perform a search
from that node up all of its parents until we reach the top of the parse stack.
This requires a data structure that knows about the parents of all nodes.

This search is performed in `find_path()`. There may be multiple paths leading
to the stack (if the same structure is repeated in multiple places). We pick the
_shortest_ path. This path may still be quite long, if the stack is deep or if
the node is found deep down in a CLVM structure. We need to compare the length
of the path against the serialized-length of the subtree. If the path is longer,
it would be a net loss to replace it with a back reference.

During serialization, we need to track what the parse-stack will look like when
deserializing, since this is part of the structure we need to search through
when finding paths to previous sub trees.
