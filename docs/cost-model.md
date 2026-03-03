## Measuring the true cost of operators

Operators with many arguments, that interact somewhat unpredictably, can be very difficult to develop a cost model for.

A block can fit 11billion cost. A significant portion of that cost is typically paid via _size_ (in bytes) and _conditions_ (like `CREATE_COIN` and `AGG_SIG_*`). The worst case scenario is that most of this cost is paid in CLVM execution, i.e. CPU time. The benchmark I've been aiming at is to keep block execution under 10 seconds. That's approximately 1 nanosecond per cost. The `ns/cost` is the main metric I'm using, and somewhere around 0.5 is good.

All of my measurements are done on a MacBook Pro M1, a RPi5 will take longer.

To run the benchmark tools:

```
cargo run --release --features openssl --bin benchmark-clvm-cost -- --threads 3 --operator <op>
```

Keep in mind that you don't want to use more threads than there are efficiency cores on your computer.

### dimensions

For 0-dimension operators, we can just run it many times and look at a distribution of how long it takes. An example of this is `g1_negate`, You can only pass in a single argument and it must be of a fixed size.

For 1-dimension operator, we can plot a line of time over number of arguments, and time over size of arguments.

And so on. For 3 dimensional, such as `modpow` it's a bit more difficult. We can plot a heatmap for 2 arguments at a time, but the 3rd is always fixed, so we can only see slices through the 3d space. We can have multiple slices along each axis. For modpow I picked a few. Typically the results from the first few can inform other slices that are interesting to look at.

## New cost model (`NEW_COST_MODEL`)

### add / subtract

The old cost model charges per-byte based on each argument's atom size independently:

```
cost = BASE_COST
    + n_args * COST_PER_ARG
    + sum(atom_len(arg_i)) * COST_PER_BYTE
```

The new cost model instead tracks the accumulator and charges per-step based on the magnitude of the numbers involved:

```
cost = BASE_COST + sum over each operator application:
    COST_PER_ARG + max(accumulator.limbs, arg.limbs) * COST_PER_BYTE
```

The key change is using `max(accumulator, argument)` rather than summing
atom lengths. Addition of two bigints takes time proportional to the
longer operand (iterating limbs with carry propagation), so `max` is the
right model. The old formula, which summed raw atom lengths, could
over-charge for many small arguments and under-charge when a large
accumulator dominates.

The first argument is special, it doesn't carry any cost since it's just loading
the accumulator, which will be charged for when the addition/subtraction
operation is applied to it.

Note that `limbs` is the magnitude of the _value_ (like `Number::bits().div_ceil(8)`), not the atom encoding length. This means leading zero bytes in the atom representation don't inflate the cost.

### multiply

```
cost = MUL_BASE_COST + sum over each pair (accumulator, arg):
    MUL_COST_PER_OP
    + (l0 + l1) * MUL_LINEAR_COST_PER_BYTE
    + (l0 * l1) / MUL_SQUARE_COST_DIVIDER
```

Where `l0` is the accumulator magnitude and `l1` is the next argument's magnitude.

The formula has the same shape as the old cost model, but the base cost and the quadratic divisor are adjusted based on measurements. The quadratic term `(l0 * l1)` reflects that bigint multiplication is fundamentally O(n\*m) in operand sizes.

### divmod

All 3 operators in the division family (`div`, `divmod` and `mod`) have the same cost.

a0 = dividend magnitude
a1 = divisor magnitude

```
cost = DIV_BASE_COST
    + (a0 + a1) * DIV_LINEAR_COST
    + (a0 * a1) / DIV_SQUARE_DIVIDER
```

### modpow

The CLVM cost model for `modpow` is:

m = modulus magnitude
e = exponent magnitude
b = base magnitude

```
cost = MODPOW_BASE_COST
    + e * EXPONENT_MULTIPLIER * (m^2 + PER_ITERATION_COST)
    + b * m
```

The dominant term is `e * m^2` which reflects the modular exponentiation algorithm: `e` controls the number of squaring/multiplication iterations, and each modular multiplication is O(m^2).

### bls_pairing_identity / bls_verify

```
cost = BLS_PAIRING_BASE_COST + n_pairs * BLS_PAIRING_COST_PER_ARG
```

For `bls_verify`, each pair additionally includes per-byte costs for hashing the message to G2 (via `map_to_g2`).

The old cost model used a relatively high base cost and low per-argument cost. Measurements showed that the marginal time per additional pairing argument (~2.1M ns) was far higher than the old per-argument cost reflected, causing `ns/cost` to rise steadily with more arguments. The new model lowers the base cost and raises the per-argument cost to keep the ratio flat across argument counts.

### logand / logior / logxor

The cost formula is `BASE + n_args * PER_ARG + effective_bytes * PER_BYTE`, with the same constants as the old model. What changed is how `effective_bytes` is computed for negative arguments.

In the old model, negative arguments contribute their raw atom length to `effective_bytes`, and positive and negative values are accumulated separately then combined at the end. In the new model, negative arguments are folded into the positive accumulator immediately, and their effective length is `max(atom_len, accumulator.limbs)`. This better reflects that bitwise operations on a small negative number and a large accumulator still touch all limbs of the accumulator (because negative numbers are sign-extended).

### Operators with only constant changes

The following operators kept the same cost formula shape but had their constants re-tuned based on measurements:

- `sha256` — `base + n_args * per_arg + total_bytes * per_byte`
- `keccak256` — `base + n_args * per_arg + total_bytes * per_byte`
- `>` (greater-than) — `base + (len0 + len1) * per_byte`
- `g1_multiply` — `base + scalar_len * per_byte`
- `g2_multiply` — `base + scalar_len * per_byte`
- `g1_map` — `base + msg_len * per_byte + dst_len * dst_per_byte`
- `g2_map` — `base + msg_len * per_byte + dst_len * dst_per_byte`
- `i` (if) — flat cost
- `l` (listp) — flat cost
- `substr` — flat cost
- `coinid` — flat cost (derived from sha256 constants)
