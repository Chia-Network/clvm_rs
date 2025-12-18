# Information on the new (sha256tree) operator

Adding `sha256tree` as a native operator made sense as it is one of the most common functions, used in nearly every shipped ChiaLisp puzzle. 
Furthermore it has an innate inefficiency in it's in-language implementation.
Every internal hash is allocated as an atom in clvm_rs allocator.
In addition to this, a native operator also opens the door to future optimisations via caching.

## Costing Goals

The matter of how to assign Cost to the new operator was the subject of intense thought and debate.
It should be costed in proportion to:
- the time to Cost vs other operators, and especially the `sha256` operator
- the time to Cost ratio of the in-language implementation 
- the its inputs

The lattermost being a unique problem with regards to shatree as it is the first operator that parses trees, so utmost care must be taken when assigning cost.

One final consideration while costing is that we required it to tally the cost during the runtime, rather than afterwards - which would be the most efficient calculation. This is because we want the oepration to fail immediately if `max_cost` is exceeded.

## Costing Methodology

The `BASE_COST` was set to equal the base cost of `sha256`.

The `COST_PER_BYTES32` was designed as the sha256 operation operates on 32byte chunks. We set the Cost to be on parity with the Cost of `sha256` although sha256 costs `per byte` and `per arg`. 
We can ignore `per arg` as `sha256tree` only takes a single argument, and we benchmarked the `cost-per-bytes32` so that it matches `sha256`'s `cost-per-byte`.

Finally the `COST_PER_NODE` was the trickiest to pin down as it is the most unique to this operator.
The trick to costing was to compare with the "in-language" implementation and deduct the costs of the known hash operations using our previously costed `COST_PER_BYTES32`.

The calculations for this can be seen in the file `sha256tree-benching.rs`.

`MacOS M1`
```
Costs based on an increasing atom per bytes32 chunks: 
Native time per bytes32  (ns): 95.1425
CLVM   time per bytes32  (ns): 94.9895
Native implementation takes 100.1611% of the time.
Native (time_per_bytes32  * cost_ratio): 611.3642
CLVM   (time_per_bytes  * cost_ratio) : 610.3807
Native cost per bytes32      : 64.0000
CLVM   cost per bytes32      : 64.0000
100.1611% of the CLVM cost is:  : 64.1031

Costs based on growing a balanced binary tree: 
Native time per node  (ns): 203.8718
CLVM   time per node  (ns): 517.8038
Native implementation takes 39.3724% of the time.
Native (time_per_node  * cost_ratio): 1310.0339
CLVM   (time_per_node  * cost_ratio) : 3327.2886
Native cost per node      : 564.0000
CLVM   cost per node      : 1463.0000
39.3724% of the CLVM cost is:  : 576.0185

Costs based on growing a list: 
Native time per node  (ns): 115.0891
CLVM   time per node  (ns): 397.1927
Native implementation takes 28.9756% of the time.
Native (time_per_node  * cost_ratio): 739.5365
CLVM   (time_per_node * cost_ratio): 2552.2694
Native cost per node      : 500.0000
CLVM   cost per node      : 1399.0000
28.9756% of the CLVM cost is:  : 405.3693
```

`Windows`
```
Costs based on an increasing atom per bytes32 chunks:
Native time per bytes32  (ns): 10.4049
CLVM   time per bytes32  (ns): 10.2604
Native implementation takes 101.4084% of the time.
Native (time_per_bytes32  * cost_ratio): 66.8597
CLVM   (time_per_bytes  * cost_ratio) : 65.9311
Native cost per bytes32      : 64.0000
CLVM   cost per bytes32      : 64.0000
101.4084% of the CLVM cost is:  : 64.9014

Costs based on growing a balanced binary tree:
Native time per node  (ns): 62.6417
CLVM   time per node  (ns): 1350.7339
Native implementation takes 4.6376% of the time.
Native (time_per_node  * cost_ratio): 402.5212
CLVM   (time_per_node  * cost_ratio) : 8679.5078
Native cost per node      : 564.0000
CLVM   cost per node      : 1463.0000
4.6376% of the CLVM cost is:  : 67.8481

Costs based on growing a list:
Native time per node  (ns): 61.1526
CLVM   time per node  (ns): 608.9923
Native implementation takes 10.0416% of the time.
Native (time_per_node  * cost_ratio): 392.9526
CLVM   (time_per_node * cost_ratio): 3913.2455
Native cost per node      : 500.0000
CLVM   cost per node      : 1399.0000
10.0416% of the CLVM cost is:  : 140.4821
```