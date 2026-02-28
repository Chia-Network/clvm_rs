use chia_bls::{PublicKey, hash_to_g2};
use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::error::EvalErr;
use clvmr::run_program::run_program;
use clvmr::{ChiaDialect, ClvmFlags};
use hex_literal::hex;
use rand::{RngCore, SeedableRng, rngs::StdRng};
use std::fs::{File, create_dir_all};
use std::io::Write;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

// We're just interested in nanoseconds / cost. There's no need to have samples
// take the full max block cost
const COST_LIMIT: u64 = 11_000_000_000;

// On a normal, consumer, machine, we aim for operators to stay below 0.5 ns
// per cost. This gives us some head room for slower mahcines, like RPi5
const BENCHMARK_TIME_PER_COST: f64 = 0.5;

#[derive(Clone, Copy)]
enum AtomFill {
    Random,
    Ones,
}

#[derive(Clone, Copy)]
enum PointKind {
    Random,
    Fixed,
    FixedNeg,
}

enum ParamDef {
    Bytes {
        name: &'static str,
        size: Range<i64>,
        fixed: &'static [i64],
        fill: AtomFill,
    },
    G1Point {
        name: &'static str,
        kind: PointKind,
    },
    G2Point {
        name: &'static str,
        kind: PointKind,
    },
    FixedAtom {
        name: &'static str,
        data: &'static [u8],
    },
}

impl ParamDef {
    fn name(&self) -> &'static str {
        match self {
            ParamDef::Bytes { name, .. }
            | ParamDef::G1Point { name, .. }
            | ParamDef::G2Point { name, .. }
            | ParamDef::FixedAtom { name, .. } => name,
        }
    }

    fn sizes(&self, steps: usize) -> Vec<i64> {
        match self {
            ParamDef::Bytes { size, .. } => {
                let range = size.end - size.start;
                let step = range.max(1) / steps.max(1) as i64;
                let step = step.max(1);
                let mut result = Vec::new();
                let mut v = size.start;
                while v <= size.end {
                    if v != 0 {
                        result.push(v);
                    }
                    v += step;
                }
                result.reverse();
                result
            }
            ParamDef::G1Point { .. } => vec![48],
            ParamDef::G2Point { .. } => vec![96],
            ParamDef::FixedAtom { data, .. } => vec![data.len() as i64],
        }
    }

    fn fixed(&self) -> i64 {
        match self {
            ParamDef::Bytes { fixed, .. } => fixed[0],
            ParamDef::G1Point { .. } => 48,
            ParamDef::G2Point { .. } => 96,
            ParamDef::FixedAtom { data, .. } => data.len() as i64,
        }
    }

    fn fixed_values(&self) -> &[i64] {
        match self {
            ParamDef::Bytes { fixed, .. } => fixed,
            ParamDef::G1Point { .. } => &[48],
            ParamDef::G2Point { .. } => &[96],
            ParamDef::FixedAtom { .. } => &[],
        }
    }

    fn size_range_desc(&self) -> String {
        match self {
            ParamDef::Bytes { size, .. } => {
                format!("{}..{}B", size.start, size.end)
            }
            ParamDef::G1Point { .. } => "G1".to_string(),
            ParamDef::G2Point { .. } => "G2".to_string(),
            ParamDef::FixedAtom { data, .. } => format!("{}B", data.len()),
        }
    }

    fn is_sweepable(&self) -> bool {
        match self {
            ParamDef::Bytes { size, .. } => size.start != size.end,
            _ => false,
        }
    }
}

struct OpDef {
    name: &'static str,
    opcode: u32,
    steps: usize,
    params: &'static [ParamDef],
    /// Max number of args for the variadic benchmark (0 = not variadic).
    /// The `params` sequence is repeated cyclically to fill additional args.
    variadic: usize,
}

const OPERATORS: &[OpDef] = &[
    OpDef {
        name: "any",
        opcode: 33,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: 1..1000,
            fixed: &[250],
            fill: AtomFill::Random,
        }],
        variadic: 100,
    },
    OpDef {
        name: "all",
        opcode: 34,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: 1..1000,
            fixed: &[250],
            fill: AtomFill::Random,
        }],
        variadic: 100,
    },
    OpDef {
        name: "if-true",
        opcode: 3,
        steps: 200,
        params: &[
            ParamDef::FixedAtom {
                name: "condition",
                data: &[1],
            },
            ParamDef::FixedAtom {
                name: "a",
                data: &[1, 2, 3, 4, 5],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[5, 4, 3, 2, 1],
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "if-false",
        opcode: 3,
        steps: 200,
        params: &[
            ParamDef::FixedAtom {
                name: "condition",
                data: &[],
            },
            ParamDef::FixedAtom {
                name: "a",
                data: &[1, 2, 3, 4, 5],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[5, 4, 3, 2, 1],
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "listp",
        opcode: 7,
        steps: 200,
        params: &[ParamDef::FixedAtom {
            name: "input",
            data: &[1],
        }],
        variadic: 0,
    },
    OpDef {
        name: "not",
        opcode: 32,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: 1..1000,
            fixed: &[25000],
            fill: AtomFill::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "strlen",
        opcode: 13,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: 1..100_000_000,
            fixed: &[25000],
            fill: AtomFill::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "sha256",
        opcode: 11,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: 1..100_000_000,
            fixed: &[1],
            fill: AtomFill::Random,
        }],
        variadic: 1000,
    },
    OpDef {
        name: "add",
        opcode: 16,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[25000],
                fill: AtomFill::Ones,
            },
            ParamDef::Bytes {
                name: "b",
                size: -10_000_000..10_000_000,
                fixed: &[25000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 550,
    },
    OpDef {
        name: "subtract",
        opcode: 17,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
            ParamDef::Bytes {
                name: "b",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 550,
    },
    OpDef {
        name: "multiply",
        opcode: 18,
        steps: 20,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -400_000..400_000,
                fixed: &[8000, 2000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "b",
                size: -400_000..400_000,
                fixed: &[175000, 8],
                fill: AtomFill::Random,
            },
        ],
        variadic: 30,
    },
    OpDef {
        name: "div",
        opcode: 19,
        steps: 15,
        params: &[
            ParamDef::Bytes {
                name: "dividend",
                size: -400_000..400_000,
                fixed: &[340_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "divisor",
                size: -400_000..400_000,
                fixed: &[30_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "divmod",
        opcode: 20,
        steps: 15,
        params: &[
            ParamDef::Bytes {
                name: "dividend",
                size: -400_000..400_000,
                fixed: &[340_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "divisor",
                size: -400_000..400_000,
                fixed: &[30_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "mod",
        opcode: 61,
        steps: 15,
        params: &[
            ParamDef::Bytes {
                name: "dividend",
                size: -400_000..400_000,
                fixed: &[340_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "divisor",
                size: -400_000..400_000,
                fixed: &[30_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "pubkey_for_exp",
        opcode: 30,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "exp",
            size: 1..10_000_000,
            fixed: &[1],
            fill: AtomFill::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "bls_map_to_g1",
        opcode: 56,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "exp",
            size: 1..10_000_000,
            fixed: &[1],
            fill: AtomFill::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "bls_map_to_g2",
        opcode: 57,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "exp",
            size: 1..10_000_000,
            fixed: &[1],
            fill: AtomFill::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "point_add",
        opcode: 29,
        steps: 1000,
        params: &[
            ParamDef::G1Point {
                name: "a",
                kind: PointKind::Random,
            },
            ParamDef::G1Point {
                name: "b",
                kind: PointKind::Random,
            },
        ],
        variadic: 550,
    },
    OpDef {
        name: "bls_g1_subtract",
        opcode: 49,
        steps: 1000,
        params: &[
            ParamDef::G1Point {
                name: "a",
                kind: PointKind::Random,
            },
            ParamDef::G1Point {
                name: "b",
                kind: PointKind::Random,
            },
        ],
        variadic: 550,
    },
    OpDef {
        name: "bls_g1_multiply",
        opcode: 50,
        steps: 100,
        params: &[
            ParamDef::G1Point {
                name: "point",
                kind: PointKind::Random,
            },
            ParamDef::Bytes {
                name: "scalar",
                size: 1..10_000_000,
                fixed: &[1],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "bls_g1_negate",
        opcode: 51,
        steps: 1000,
        params: &[ParamDef::G1Point {
            name: "point",
            kind: PointKind::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "bls_g2_negate",
        opcode: 55,
        steps: 1000,
        params: &[ParamDef::G2Point {
            name: "point",
            kind: PointKind::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "bls_g2_add",
        opcode: 52,
        steps: 1000,
        params: &[
            ParamDef::G2Point {
                name: "a",
                kind: PointKind::Random,
            },
            ParamDef::G2Point {
                name: "b",
                kind: PointKind::Random,
            },
        ],
        variadic: 550,
    },
    OpDef {
        name: "bls_g2_subtract",
        opcode: 53,
        steps: 1000,
        params: &[
            ParamDef::G2Point {
                name: "a",
                kind: PointKind::Random,
            },
            ParamDef::G2Point {
                name: "b",
                kind: PointKind::Random,
            },
        ],
        variadic: 550,
    },
    OpDef {
        name: "bls_g2_multiply",
        opcode: 54,
        steps: 100,
        params: &[
            ParamDef::G2Point {
                name: "point",
                kind: PointKind::Random,
            },
            ParamDef::Bytes {
                name: "scalar",
                size: 1..10_000_000,
                fixed: &[1],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "bls_pairing_identity",
        opcode: 58,
        steps: 1000,
        params: &[
            ParamDef::G1Point {
                name: "g1",
                kind: PointKind::Fixed,
            },
            ParamDef::G2Point {
                name: "g2",
                kind: PointKind::Fixed,
            },
            ParamDef::G1Point {
                name: "-g1",
                kind: PointKind::FixedNeg,
            },
            ParamDef::G2Point {
                name: "g2b",
                kind: PointKind::Fixed,
            },
        ],
        variadic: 600,
    },
    OpDef {
        name: "lognot",
        opcode: 27,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: -100..10_000_000,
            fixed: &[5_000_000],
            fill: AtomFill::Random,
        }],
        variadic: 0,
    },
    OpDef {
        name: "logand",
        opcode: 24,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "b",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "logior",
        opcode: 25,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "b",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "logxor",
        opcode: 26,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "b",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "logand-multi",
        opcode: 24,
        steps: 50,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "logior-multi",
        opcode: 25,
        steps: 50,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "logxor-multi",
        opcode: 26,
        steps: 50,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: -10_000_000..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
            ParamDef::FixedAtom {
                name: "b",
                data: &[0xff],
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "gr",
        opcode: 21,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: 1..30_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
            ParamDef::Bytes {
                name: "b",
                size: 1..30_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "gr_bytes",
        opcode: 10,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
            ParamDef::Bytes {
                name: "b",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "eq",
        opcode: 9,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
            ParamDef::Bytes {
                name: "b",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Ones,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "ash",
        opcode: 22,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "value",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "shift",
                size: 1..2,
                fixed: &[1],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "lsh",
        opcode: 23,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "value",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "shift",
                size: 1..2,
                fixed: &[1],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "concat",
        opcode: 14,
        steps: 30,
        params: &[
            ParamDef::Bytes {
                name: "a",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "b",
                size: 1..10_000_000,
                fixed: &[5_000_000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 30,
    },
    OpDef {
        name: "substr",
        opcode: 12,
        steps: 200,
        params: &[
            ParamDef::Bytes {
                name: "string",
                size: 1..50_000_000,
                fixed: &[1000],
                fill: AtomFill::Random,
            },
            ParamDef::FixedAtom {
                name: "start",
                data: &[],
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "keccak256",
        opcode: 62,
        steps: 100,
        params: &[ParamDef::Bytes {
            name: "input",
            size: 1..10_000_000,
            fixed: &[1],
            fill: AtomFill::Random,
        }],
        variadic: 600,
    },
    OpDef {
        name: "coinid",
        opcode: 48,
        steps: 1000,
        params: &[
            ParamDef::Bytes {
                name: "parent",
                size: 32..32,
                fixed: &[32],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "puzzle_hash",
                size: 32..32,
                fixed: &[32],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "amount",
                size: 1..8,
                fixed: &[8],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "modpow",
        opcode: 60,
        steps: 15,
        params: &[
            ParamDef::Bytes {
                name: "base",
                size: 1..10_000,
                fixed: &[8, 9000],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "exponent",
                size: 1..10_000,
                fixed: &[1, 50, 500],
                fill: AtomFill::Random,
            },
            ParamDef::Bytes {
                name: "modulus",
                size: 1..6000,
                fixed: &[8, 1000],
                fill: AtomFill::Random,
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "secp256k1_verify",
        opcode: 64,
        steps: 1000,
        params: &[
            ParamDef::FixedAtom {
                name: "pubkey",
                data: &hex!("02390b19842e100324163334b16947f66125b76d4fa4a11b9ccdde9b7398e64076"),
            },
            ParamDef::FixedAtom {
                name: "msg",
                data: &hex!("85932e4d075615be881398cc765f9f78204033f0ef5f832ac37e732f5f0cbda2"),
            },
            ParamDef::FixedAtom {
                name: "sig",
                data: &hex!(
                    "481477e62a1d02268127ae89cc58929e09ad5d30229721965ae35965d098a5f630205a7e69f4cb8084f16c7407ed7312994ffbf87ba5eb1aee16682dd324943e"
                ),
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "secp256r1_verify",
        opcode: 65,
        steps: 1000,
        params: &[
            ParamDef::FixedAtom {
                name: "pubkey",
                data: &hex!("033e1a1b2ccbc35883c60fdfc3f4a02175096ade6271fe85517ca5772594bbd0dc"),
            },
            ParamDef::FixedAtom {
                name: "msg",
                data: &hex!("85932e4d075615be881398cc765f9f78204033f0ef5f832ac37e732f5f0cbda2"),
            },
            ParamDef::FixedAtom {
                name: "sig",
                data: &hex!(
                    "eae2f488080919bd0a7069c24cdd9c6ce2db423861b0c9d4236cdadbd0005f6d8f3709e6eb19249fd9c8bea664aba35218e67ea4b0f2239488dc3147f336e1e6"
                ),
            },
        ],
        variadic: 0,
    },
    OpDef {
        name: "bls_verify",
        opcode: 59,
        steps: 1000,
        params: &[
            ParamDef::FixedAtom {
                name: "sig",
                data: &hex!(
                    "b19d22fcdd0370a3e7abb2758345a4c49eb47609f30d341a6c414bf610f895a2"
                    "aa14de855eaf72496d71bdc0dbc894650bf49dfac4cb919426e530c1c18f5752"
                    "5792a8053ed18f0ec0659b7d575d409f0485cc3abb8b66b23c057288604d694b"
                ),
            },
            ParamDef::FixedAtom {
                name: "pk1",
                data: &hex!(
                    "8d84cd1c33f37c64b864f8c042b6a11298f42618b4dfafb7ce43bd5142bd3022"
                    "32d096baf05c6ea6dc090ed8576fc7ab"
                ),
            },
            ParamDef::FixedAtom {
                name: "msg1",
                data: &hex!("7e0411e0b4585734180ed0248d8dfbadbaac4c"),
            },
            ParamDef::FixedAtom {
                name: "pk2",
                data: &hex!(
                    "b997e2df3d201d078ffb2f11f98f9256903c4cee1f776023432984be15b3acbe"
                    "8f22b8ccd21d510c020b02b0058100d7"
                ),
            },
            ParamDef::FixedAtom {
                name: "msg2",
                data: &hex!(
                    "a8ae58ddb256fa7a369aaca97faab7672ed39ca8db15f5c6f8be930c8859cb41b6ced6a1bb4b"
                ),
            },
            ParamDef::FixedAtom {
                name: "pk3",
                data: &hex!(
                    "8e63403e730756a7ed5e0a26ec2ee94b3591f3452b907fe42e81af6104dbb331"
                    "175a2ac88b77c43a7ae65f73384867df"
                ),
            },
            ParamDef::FixedAtom {
                name: "msg3",
                data: &hex!("dbf08606e525f72b9c7483847dc6e351081a37c5bbda1e"),
            },
        ],
        variadic: 0,
    },
];

/// Benchmark CLVM operator costs and validate the cost model. This tool is not
/// measuring what the cost model should be, it just validates the existing
/// model by computing the CPU time spent per cost point. It's measured in
/// nanoseconds per cost.
#[derive(Parser)]
struct Args {
    /// Output directory (defaults to measurements-v2 with --new-cost-model)
    #[arg(long)]
    output_dir: Option<String>,

    /// Only run the named operator(s) (runs all if omitted)
    #[arg(long)]
    operator: Vec<String>,

    /// List available operators and exit
    #[arg(long)]
    list: bool,

    /// Perform higher resolution sampling of operators
    #[arg(long)]
    hires: bool,

    /// Number of parallel measurement threads
    #[arg(long, default_value_t = 1)]
    threads: usize,

    /// Use the new cost model for benchmarking
    #[arg(long)]
    new_cost_model: bool,

    /// Regenerate gnuplot scripts from existing data files without re-running benchmarks
    #[arg(long)]
    render_only: bool,
}

fn format_fixed_params(op: &OpDef, fixed_params: &[(usize, i64)]) -> String {
    if fixed_params.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = fixed_params
        .iter()
        .map(|&(pi, sz)| format!("{}={}B", op.params[pi].name(), sz))
        .collect();
    format!(", {}", parts.join(", "))
}

fn fixed_file_suffix(op: &OpDef, fixed_params: &[(usize, i64)]) -> String {
    fixed_params
        .iter()
        .map(|&(pi, sz)| {
            let sign = if sz < 0 { "n" } else { "" };
            format!("-{}={sign}{:x}", op.params[pi].name(), sz.unsigned_abs())
        })
        .collect::<String>()
}

fn format_size(bytes: i64) -> String {
    let abs_bytes = bytes.unsigned_abs() as f64;
    let sign = if bytes < 0 { "-" } else { "" };
    if abs_bytes >= 10000.0 {
        format!("{sign}{:.0}K", abs_bytes / 1024.0)
    } else if abs_bytes >= 1024.0 {
        format!("{sign}{:.1}K", abs_bytes / 1024.0)
    } else {
        format!("{sign}{}", bytes.unsigned_abs())
    }
}

fn quote(a: &mut Allocator, v: NodePtr) -> NodePtr {
    a.new_pair(a.one(), v).unwrap()
}

fn random_atom(a: &mut Allocator, size: usize, negative: bool, rng: &mut StdRng) -> NodePtr {
    if size == 0 {
        return a.one();
    }
    let mut buf = vec![0u8; size];
    rng.fill_bytes(&mut buf);
    if negative {
        buf[0] |= 0x80;
    } else {
        buf[0] &= 0x7f;
        if buf.iter().all(|&b| b == 0) {
            buf[size - 1] = 1;
        }
    }
    a.new_atom(&buf).unwrap()
}

fn ones_atom(a: &mut Allocator, size: usize) -> NodePtr {
    if size == 0 {
        return a.one();
    }
    let buf = vec![1u8; size];
    a.new_atom(&buf).unwrap()
}

fn random_g1_atom(a: &mut Allocator, rng: &mut StdRng) -> NodePtr {
    let mut scalar = [0u8; 32];
    rng.fill_bytes(&mut scalar);
    let g1 = PublicKey::from_integer(&scalar);
    a.new_g1(g1).unwrap()
}

fn fixed_g1_atom(a: &mut Allocator, negate: bool) -> NodePtr {
    let mut scalar = [0u8; 32];
    scalar[31] = 1;
    let g1 = PublicKey::from_integer(&scalar);
    a.new_g1(if negate { -g1 } else { g1 }).unwrap()
}

fn random_g2_atom(a: &mut Allocator, rng: &mut StdRng) -> NodePtr {
    let mut msg = [0u8; 32];
    rng.fill_bytes(&mut msg);
    let g2 = hash_to_g2(&msg);
    a.new_g2(g2).unwrap()
}

fn fixed_g2_atom(a: &mut Allocator, negate: bool) -> NodePtr {
    let g2 = hash_to_g2(&[1, 2, 3]);
    a.new_g2(if negate { -g2 } else { g2 }).unwrap()
}

enum MeasureResult {
    Success {
        ns_per_cost: f64,
        elapsed_ns: f64,
        cost: u64,
    },
    CostExceeded,
    OtherFailure,
}

fn make_atom(a: &mut Allocator, pdef: &ParamDef, rng: &mut StdRng) -> NodePtr {
    make_atom_sized(a, pdef, pdef.fixed(), rng)
}

fn make_atom_sized(a: &mut Allocator, pdef: &ParamDef, sz: i64, rng: &mut StdRng) -> NodePtr {
    match pdef {
        ParamDef::Bytes { fill, .. } => {
            let byte_count = sz.unsigned_abs() as usize;
            match fill {
                AtomFill::Random => random_atom(a, byte_count, sz < 0, rng),
                AtomFill::Ones => ones_atom(a, byte_count),
            }
        }
        ParamDef::G1Point { kind, .. } => match kind {
            PointKind::Random => random_g1_atom(a, rng),
            PointKind::Fixed => fixed_g1_atom(a, false),
            PointKind::FixedNeg => fixed_g1_atom(a, true),
        },
        ParamDef::G2Point { kind, .. } => match kind {
            PointKind::Random => random_g2_atom(a, rng),
            PointKind::Fixed => fixed_g2_atom(a, false),
            PointKind::FixedNeg => fixed_g2_atom(a, true),
        },
        ParamDef::FixedAtom { data, .. } => a.new_atom(data).unwrap(),
    }
}

fn run_clvm_op(
    a: &mut Allocator,
    opcode: u32,
    atoms: &[NodePtr],
    flags: ClvmFlags,
) -> MeasureResult {
    let mut args = NodePtr::NIL;
    for &atom in atoms.iter().rev() {
        let q = quote(a, atom);
        args = a.new_pair(q, args).unwrap();
    }

    let opcode_node = a.new_number(opcode.into()).unwrap();
    let call = a.new_pair(opcode_node, args).unwrap();

    let dialect = ChiaDialect::new(
        flags
            | ClvmFlags::ENABLE_SHA256_TREE
            | ClvmFlags::RELAXED_BLS
            | ClvmFlags::ENABLE_KECCAK_OPS_OUTSIDE_GUARD
            | ClvmFlags::ENABLE_SECP_OPS,
    );
    let start = Instant::now();
    let result = run_program(a, &dialect, call, NodePtr::NIL, COST_LIMIT);
    let elapsed_ns = start.elapsed().as_nanos() as f64;

    match result {
        Ok(reduction) => {
            let cost = reduction.0;
            let ns_per_cost = elapsed_ns / cost as f64;
            assert!(
                ns_per_cost > 0.0,
                "ns_per_cost must be positive. elapsed: {elapsed_ns} ns cost: {cost}",
            );
            MeasureResult::Success {
                ns_per_cost,
                elapsed_ns,
                cost,
            }
        }
        Err(EvalErr::CostExceeded) => MeasureResult::CostExceeded,
        Err(_) => MeasureResult::OtherFailure,
    }
}

fn warmup_op(op: &OpDef, flags: ClvmFlags) {
    let mut rng = StdRng::seed_from_u64(0);
    let mut a = Allocator::new();
    let atoms: Vec<NodePtr> = op
        .params
        .iter()
        .map(|pdef| make_atom(&mut a, pdef, &mut rng))
        .collect();
    let _ = run_clvm_op(&mut a, op.opcode, &atoms, flags);
}

fn measure_op(
    op: &OpDef,
    param_sizes: &[i64],
    rng: &mut StdRng,
    flags: ClvmFlags,
) -> MeasureResult {
    let mut a = Allocator::new();
    let atoms: Vec<NodePtr> = op
        .params
        .iter()
        .zip(param_sizes.iter())
        .map(|(pdef, &sz)| make_atom_sized(&mut a, pdef, sz, rng))
        .collect();
    run_clvm_op(&mut a, op.opcode, &atoms, flags)
}

fn measure_op_variadic(
    op: &OpDef,
    num_args: usize,
    rng: &mut StdRng,
    flags: ClvmFlags,
) -> MeasureResult {
    let mut a = Allocator::new();
    let n = op.params.len();
    let atoms: Vec<NodePtr> = (0..num_args)
        .map(|i| {
            let pdef = &op.params[i % n];
            make_atom(&mut a, pdef, rng)
        })
        .collect();
    run_clvm_op(&mut a, op.opcode, &atoms, flags)
}

/// Run measurements across `num_threads` threads with interleaved work
/// assignment. `work` maps a flat index to (key, MeasureResult).
fn run_parallel<K: Send>(
    total: usize,
    num_threads: usize,
    work: impl Fn(usize, &mut StdRng) -> K + Sync,
) -> Vec<K> {
    let progress = AtomicUsize::new(0);
    let progress = &progress;
    let last_print_ms = AtomicUsize::new(0);
    let last_print_ms = &last_print_ms;
    let start_time = Instant::now();
    let work = &work;

    std::thread::scope(|s| {
        let mut handles = Vec::<_>::with_capacity(num_threads);
        for t in 0..num_threads {
            let thread_fun = move || {
                let mut rng = StdRng::seed_from_u64(t as u64);
                let mut results = Vec::new();
                let mut i = t;
                while i < total {
                    let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                    let now_ms = start_time.elapsed().as_millis() as usize;
                    let prev = last_print_ms.load(Ordering::Relaxed);
                    if (done == total || now_ms.saturating_sub(prev) >= 300)
                        && last_print_ms
                            .compare_exchange(prev, now_ms, Ordering::Relaxed, Ordering::Relaxed)
                            .is_ok()
                    {
                        eprint!("\r  [{done}/{total}]   ");
                    }
                    results.push(work(i, &mut rng));
                    i += num_threads;
                }
                results
            };
            handles.push(s.spawn(thread_fun));
        }

        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

// ============================================================================
// Gnuplot script helpers
// ============================================================================

fn gnuplot_escape(s: &str) -> String {
    s.to_string()
}

// Piecewise-linear palette used by all ns/cost plots (defined in log10 space).
const CB_MIN: f64 = 0.01;
const CB_MAX: f64 = 100.0;

// Palette defined as (ns_per_cost_value, r, g, b). Positions are in absolute
// ns_per_cost space but mapped to [0,1] fractions via log scale for both
// gnuplot's palette definition and the Rust RGB conversion.
const PALETTE: &[(f64, f64, f64, f64)] = &[
    (0.01, 0.0, 0.25, 0.0),
    (0.32, 0.3, 0.7, 0.0),
    (0.40, 0.5, 0.85, 0.0),
    (0.48, 1.0, 1.0, 0.0),
    (1.0, 1.0, 0.65, 0.0),
    (2.0, 1.0, 0.3, 0.0),
    (3.2, 0.9, 0.0, 0.0),
    (6.3, 0.7, 0.0, 0.3),
    (10.0, 0.2, 0.0, 1.0),
    (100.0, 0.0, 0.0, 0.0),
];

/// Map an absolute ns_per_cost value to a [0,1] fraction via log scale.
fn cb_fraction(v: f64) -> f64 {
    (v.log10() - CB_MIN.log10()) / (CB_MAX.log10() - CB_MIN.log10())
}

/// Map ns_per_cost to an RGB integer via the palette (interpolated in log space).
fn palette_to_rgb(ns_per_cost: f64) -> u32 {
    let f = cb_fraction(ns_per_cost.clamp(CB_MIN, CB_MAX));
    let fracs: Vec<f64> = PALETTE.iter().map(|(v, _, _, _)| cb_fraction(*v)).collect();
    let mut i = 1;
    while i < fracs.len() && fracs[i] < f {
        i += 1;
    }
    if i >= fracs.len() {
        i = fracs.len() - 1;
    }
    let t = if (fracs[i] - fracs[i - 1]).abs() < 1e-12 {
        0.0
    } else {
        (f - fracs[i - 1]) / (fracs[i] - fracs[i - 1])
    };
    let (_, r0, g0, b0) = PALETTE[i - 1];
    let (_, r1, g1, b1) = PALETTE[i];
    let r = ((r0 + t * (r1 - r0)) * 255.0) as u32;
    let g = ((g0 + t * (g1 - g0)) * 255.0) as u32;
    let b = ((b0 + t * (b1 - b0)) * 255.0) as u32;
    (r << 16) | (g << 8) | b
}

/// Gnuplot `set palette defined(...)` with breakpoints as [0,1] fractions
/// matching the log-scale cb mapping. gnuplot interpolates palette breakpoints
/// linearly, so we pre-map our absolute values to log-space fractions.
fn palette_gnuplot_def() -> String {
    PALETTE
        .iter()
        .map(|(v, r, g, b)| {
            let f = cb_fraction(*v);
            format!("{f} {r} {g} {b}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn cb_ticks_gnuplot() -> String {
    let tick_values: &[f64] = &[0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 50.0, 100.0];
    tick_values
        .iter()
        .map(|&v| format!("\"{v}\" {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Common gnuplot preamble for heatmap/palette plots.
fn heatmap_preamble() -> String {
    format!(
        "set palette defined ({palette})\n\
         set cbrange [0.01:100]\n\
         set logscale cb\n\
         set cblabel 'ns / cost'\n\
         set cbtics ({ticks})\n",
        palette = palette_gnuplot_def(),
        ticks = cb_ticks_gnuplot(),
    )
}

// ============================================================================
// Data saving + gnuplot script generation for each benchmark type
// ============================================================================

fn benchmark_histogram(
    op: &OpDef,
    samples: usize,
    output_dir: &str,
    num_threads: usize,
    flags: ClvmFlags,
    render_only: bool,
    script: &mut String,
) {
    let dat_name = format!("{}-histogram.dat", op.name);
    let dat_path = format!("{output_dir}/data/{dat_name}");

    let values: Vec<f64> = if render_only {
        let path = std::path::Path::new(&dat_path);
        if !path.exists() {
            println!("  {dat_name}: no data file, skipping");
            return;
        }
        let content = std::fs::read_to_string(path).expect("failed to read data file");
        content
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .filter_map(|l| l.trim().parse::<f64>().ok())
            .collect()
    } else {
        println!("\n{}: {} samples", op.name, samples);

        let param_sizes: Vec<i64> = op.params.iter().map(|p| p.fixed()).collect();

        let results = run_parallel(samples, num_threads, |_i, rng| {
            measure_op(op, &param_sizes, rng, flags)
        });

        let mut vals = Vec::new();
        for result in results {
            if let MeasureResult::Success { ns_per_cost, .. } = result {
                vals.push(ns_per_cost);
            }
        }

        if vals.is_empty() {
            eprintln!("  no successful measurements");
            return;
        }

        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        let median = vals[vals.len() / 2];
        println!("  mean={mean:.3} ns/cost, median={median:.3} ns/cost");

        {
            let mut f = File::create(&dat_path).expect("failed to create data file");
            writeln!(
                f,
                "# {}: ns/cost distribution ({} samples)",
                op.name,
                vals.len()
            )
            .unwrap();
            writeln!(f, "# mean={mean:.6} median={median:.6}").unwrap();
            writeln!(f, "# columns: ns_per_cost").unwrap();
            for &v in &vals {
                writeln!(f, "{v}").unwrap();
            }
        }
        println!("  Saved: {dat_path}");

        vals
    };

    if values.is_empty() {
        return;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let median = values[values.len() / 2];
    let max_val = *values.last().unwrap();

    let n = values.len();
    let png_name = format!("{}-cdf.png", op.name);
    let title = gnuplot_escape(&format!("{} ns/cost CDF ({} samples)", op.name, n));
    let x_max = max_val.max(BENCHMARK_TIME_PER_COST) * 1.05;

    script.push_str(&format!(
        "\n# --- {name} CDF ---\n\
         reset\n\
         set terminal pngcairo size 1100,600\n\
         set output '{png_name}'\n\
         set title '{title}' noenhanced\n\
         set xlabel 'ns / cost'\n\
         set ylabel 'cumulative fraction'\n\
         set xrange [0:{x_max}]\n\
         set yrange [0:1.05]\n\
         N = {n}.0\n\
         set arrow nohead from {bm_time},graph 0 to {bm_time},graph 1 lc rgb '#ff0000' lw 1.5\n\
         set label '{bm_time}' at {bm_time},graph 0.97 tc rgb '#ff0000' font ',9'\n\
         set arrow nohead from {mean},0 to {mean},0.92 lc rgb '#008800' dt 2 lw 1.5\n\
         set label 'mean={mean:.3}' at {mean},0.93 tc rgb '#008800' font ',9'\n\
         set arrow nohead from {median},0 to {median},0.85 lc rgb '#cc6600' dt 2 lw 1.5\n\
         set label 'median={median:.3}' at {median},0.86 tc rgb '#cc6600' font ',9'\n\
         plot 'data/{dat_name}' using 1:($0/N) with lines lc rgb 'blue' lw 1.5 notitle\n",
        name = op.name,
        bm_time = BENCHMARK_TIME_PER_COST,
    ));
}

#[allow(clippy::too_many_arguments)]
fn benchmark_1d_slice(
    op: &OpDef,
    vary_idx: usize,
    fixed_params: &[(usize, i64)],
    steps: usize,
    output_dir: &str,
    num_threads: usize,
    flags: ClvmFlags,
    render_only: bool,
    script: &mut String,
) {
    let p = &op.params[vary_idx];
    let sizes = p.sizes(steps);
    let n = sizes.len();
    let fixed_desc = format_fixed_params(op, fixed_params);
    let fsuffix = fixed_file_suffix(op, fixed_params);
    let dat_name = format!("{}-{}{fsuffix}.dat", op.name, p.name());
    let dat_path = format!("{output_dir}/data/{dat_name}");

    let y_max: f64 = if render_only {
        let path = std::path::Path::new(&dat_path);
        if !path.exists() {
            println!("  {dat_name}: no data file, skipping");
            return;
        }
        let content = std::fs::read_to_string(path).expect("failed to read data file");
        content
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .filter_map(|l| {
                l.split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .filter(|v| !v.is_nan())
            .fold(BENCHMARK_TIME_PER_COST, f64::max)
            * 1.05
    } else {
        println!(
            "\n{}: {} ({}, {} points{})",
            op.name,
            p.name(),
            p.size_range_desc(),
            n,
            fixed_desc
        );

        let arity = op.params.len();
        let results = run_parallel(n, num_threads, |i, rng| {
            let mut param_sizes = vec![0i64; arity];
            param_sizes[vary_idx] = sizes[i];
            for &(pi, fsz) in fixed_params {
                param_sizes[pi] = fsz;
            }
            (i, measure_op(op, &param_sizes, rng, flags))
        });

        let mut values = vec![f64::NAN; n];
        let mut ns_values = vec![f64::NAN; n];
        let mut cost_values = vec![f64::NAN; n];
        let mut exceeded = vec![false; n];
        for (i, result) in results {
            match result {
                MeasureResult::Success {
                    ns_per_cost,
                    elapsed_ns,
                    cost,
                } => {
                    values[i] = ns_per_cost;
                    ns_values[i] = elapsed_ns;
                    cost_values[i] = cost as f64;
                }
                MeasureResult::CostExceeded => {
                    exceeded[i] = true;
                }
                MeasureResult::OtherFailure => {}
            }
        }

        {
            let mut f = File::create(&dat_path).expect("failed to create data file");
            writeln!(
                f,
                "# {}: {} vs {}{}",
                op.name,
                p.name(),
                p.size_range_desc(),
                fixed_desc
            )
            .unwrap();
            writeln!(f, "# columns: size ns_per_cost elapsed_ns cost exceeded").unwrap();
            for i in 0..n {
                writeln!(
                    f,
                    "{} {} {} {} {}",
                    sizes[i],
                    if exceeded[i] {
                        "NaN".to_string()
                    } else {
                        format!("{}", values[i])
                    },
                    if exceeded[i] {
                        "NaN".to_string()
                    } else {
                        format!("{}", ns_values[i])
                    },
                    if exceeded[i] {
                        "NaN".to_string()
                    } else {
                        format!("{}", cost_values[i])
                    },
                    if exceeded[i] { 1 } else { 0 },
                )
                .unwrap();
            }
        }
        println!("  Saved: {dat_path}");

        values
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(BENCHMARK_TIME_PER_COST, f64::max)
            * 1.05
    };

    // ns/cost line plot
    let fixed_line = fixed_params
        .first()
        .map(|&(pi, sz)| (sz, op.params[pi].name()));
    let title = gnuplot_escape(&format!(
        "{} ns/cost vs {}{}",
        op.name,
        p.name(),
        fixed_desc
    ));
    let png_name = format!("{}-{}{fsuffix}.png", op.name, p.name());

    script.push_str(&format!(
        "\n# --- {name} {param} ns/cost ---\n\
         reset\n\
         set terminal pngcairo size 1100,600\n\
         set output '{png_name}'\n\
         set title '{title}' noenhanced\n\
         set xlabel '{xlabel}' noenhanced\n\
         set ylabel 'ns / cost'\n\
         set yrange [0:{y_max}]\n\
         set arrow nohead from graph 0, first {bm_time} to graph 1, first {bm_time} lc rgb '#ff0000' lw 1.0\n",
        name = op.name,
        param = p.name(),
        xlabel = gnuplot_escape(p.name()),
        bm_time = BENCHMARK_TIME_PER_COST,
    ));
    if let Some((fixed_size, label)) = fixed_line {
        let caption = gnuplot_escape(&format!("{}={}", label, format_size(fixed_size)));
        script.push_str(&format!(
            "set arrow nohead from {fx},graph 0 to {fx},graph 1 lc rgb 'gray' dt 2 lw 1.5\n\
             set label '{caption}' at {fx},graph 0.95 tc rgb 'gray' font ',9'\n",
            fx = fixed_size,
        ));
    }
    script.push_str(&format!(
        "plot 'data/{dat_name}' using 1:($5==0?$2:NaN) with points lc rgb 'blue' pt 7 ps 0.5 notitle, \\\n\
              'data/{dat_name}' using ($5==1?$1:NaN):(0) with points pt 2 lc rgb 'red' ps 1.0 notitle\n",
    ));

    // wall-clock ns vs benchmark time plot
    let title2 = gnuplot_escape(&format!("{} wall-clock ns vs cost{}", op.name, fixed_desc));
    let png_name2 = format!("{}-{}{fsuffix}-ns-vs-cost.png", op.name, p.name());

    script.push_str(&format!(
        "\n# --- {name} {param} ns vs cost ---\n\
         reset\n\
         set terminal pngcairo size 1100,600\n\
         set output '{png_name2}'\n\
         set title '{title2}' noenhanced\n\
         set xlabel '{xlabel}' noenhanced\n\
         set ylabel 'nanoseconds'\n\
         set yrange [0:*]\n\
         plot 'data/{dat_name}' using 1:($5==0?$3:NaN) with points lc rgb 'blue' pt 7 ps 0.5 title 'wall-clock (ns)', \\\n\
              'data/{dat_name}' using 1:($5==0?$4*{bm_time}:NaN) with lines lc rgb 'dark-green' lw 1.5 title 'benchmark time ({bm_time} ns/cost)'\n",
        name = op.name,
        param = p.name(),
        xlabel = gnuplot_escape(p.name()),
        bm_time = BENCHMARK_TIME_PER_COST,
    ));
}

#[allow(clippy::too_many_arguments)]
fn benchmark_2d(
    op: &OpDef,
    xi_param: usize,
    yi_param: usize,
    fixed_params: &[(usize, i64)],
    steps: usize,
    output_dir: &str,
    num_threads: usize,
    flags: ClvmFlags,
    render_only: bool,
    script: &mut String,
) {
    let px = &op.params[xi_param];
    let py = &op.params[yi_param];
    let x_sizes = px.sizes(steps);
    let y_sizes = py.sizes(steps);
    let fixed_desc = format_fixed_params(op, fixed_params);
    let fsuffix = fixed_file_suffix(op, fixed_params);
    let dat_name = format!("{}-{}-{}{fsuffix}.dat", op.name, px.name(), py.name());
    let dat_path = format!("{output_dir}/data/{dat_name}");

    if render_only {
        if !std::path::Path::new(&dat_path).exists() {
            println!("  {dat_name}: no data file, skipping");
            return;
        }
    } else {
        let nx = x_sizes.len();
        let ny = y_sizes.len();

        println!(
            "\n{}: {} vs {} ({} points{})",
            op.name,
            px.name(),
            py.name(),
            nx * ny,
            fixed_desc
        );

        let arity = op.params.len();
        let results = run_parallel(nx * ny, num_threads, |flat, rng| {
            let xi = flat % nx;
            let yi = flat / nx;
            let mut param_sizes = vec![0i64; arity];
            param_sizes[xi_param] = x_sizes[xi];
            param_sizes[yi_param] = y_sizes[yi];
            for &(pi, sz) in fixed_params {
                param_sizes[pi] = sz;
            }
            (xi, yi, measure_op(op, &param_sizes, rng, flags))
        });

        let mut data = vec![vec![f64::NAN; nx]; ny];
        let mut exceeded = Vec::new();
        for (xi, yi, result) in results {
            match result {
                MeasureResult::Success { ns_per_cost, .. } => {
                    data[yi][xi] = ns_per_cost;
                }
                MeasureResult::CostExceeded => {
                    exceeded.push((xi, yi));
                }
                MeasureResult::OtherFailure => {}
            }
        }

        {
            let mut f = File::create(&dat_path).expect("failed to create data file");
            writeln!(
                f,
                "# {}: {} vs {}{}",
                op.name,
                px.name(),
                py.name(),
                fixed_desc
            )
            .unwrap();
            writeln!(f, "# columns: {} {} ns_per_cost", px.name(), py.name()).unwrap();
            for (xi, x_sz) in x_sizes.iter().enumerate() {
                if xi > 0 {
                    writeln!(f).unwrap();
                }
                for (yi, y_sz) in y_sizes.iter().enumerate() {
                    let v = data[yi][xi];
                    if v.is_nan() {
                        writeln!(f, "{x_sz} {y_sz} ?").unwrap();
                    } else {
                        writeln!(f, "{x_sz} {y_sz} {v}").unwrap();
                    }
                }
            }
        }
        println!("  Saved: {dat_path}");

        if !exceeded.is_empty() {
            let name = format!(
                "{}-{}-{}{fsuffix}-exceeded.dat",
                op.name,
                px.name(),
                py.name()
            );
            let path = format!("{output_dir}/data/{name}");
            let mut f = File::create(&path).expect("failed to create data file");
            writeln!(f, "# exceeded: {} {}", px.name(), py.name()).unwrap();
            for &(xi, yi) in &exceeded {
                writeln!(f, "{} {}", x_sizes[xi], y_sizes[yi]).unwrap();
            }
            println!("  Saved: {path}");
        }
    }

    let exceeded_file = format!(
        "{}-{}-{}{fsuffix}-exceeded.dat",
        op.name,
        px.name(),
        py.name()
    );
    let exceeded_name =
        if std::path::Path::new(&format!("{output_dir}/data/{exceeded_file}")).exists() {
            Some(exceeded_file)
        } else {
            None
        };

    let title = gnuplot_escape(&format!(
        "{} ns/cost: {} vs {}{} [x = cost-exceeded]",
        op.name,
        px.name(),
        py.name(),
        fixed_desc
    ));
    let png_name = format!("{}-{}-{}{fsuffix}.png", op.name, px.name(), py.name());

    script.push_str(&format!(
        "\n# --- {name} {xp} vs {yp} surface ---\n\
         reset\n\
         set terminal pngcairo size 1200,1000\n\
         set output '{png_name}'\n\
         set title '{title}' noenhanced\n\
         set xlabel '{xlabel}' noenhanced\n\
         set ylabel '{ylabel}' noenhanced\n\
         set zlabel 'ns / cost' noenhanced rotate by 90\n\
         {preamble}\
         set logscale z\n\
         set ztics ({zticks}) mirror\n\
         set border 4095\n\
         set hidden3d\n\
         set style data lines\n\
         set pm3d depthorder\n\
         set view 55, 300\n\
         splot 'data/{dat_name}' using 1:2:3 with lines lc palette z notitle",
        zticks = cb_ticks_gnuplot(),
        name = op.name,
        xp = px.name(),
        yp = py.name(),
        xlabel = gnuplot_escape(px.name()),
        ylabel = gnuplot_escape(py.name()),
        preamble = heatmap_preamble(),
    ));
    if let Some(ref exc_name) = exceeded_name {
        script.push_str(&format!(
            ", \\\n      'data/{exc_name}' using 1:2:({z_floor}) with points pt 2 ps 0.5 lc rgb 'red' notitle",
            z_floor = CB_MIN,
        ));
    }
    script.push('\n');
}

#[allow(clippy::too_many_arguments)]
fn benchmark_3d(
    op: &OpDef,
    sweepable: &[usize],
    always_fixed: &[(usize, i64)],
    steps: usize,
    output_dir: &str,
    num_threads: usize,
    flags: ClvmFlags,
    render_only: bool,
    script: &mut String,
) {
    assert_eq!(sweepable.len(), 3);
    let p0 = &op.params[sweepable[0]];
    let p1 = &op.params[sweepable[1]];
    let p2 = &op.params[sweepable[2]];

    let dat_name = format!(
        "{}-{}-{}-{}-3d.dat",
        op.name,
        p0.name(),
        p1.name(),
        p2.name()
    );
    let dat_path = format!("{output_dir}/data/{dat_name}");
    let exceeded_name = format!(
        "{}-{}-{}-{}-3d-exceeded.dat",
        op.name,
        p0.name(),
        p1.name(),
        p2.name()
    );
    let exceeded_path = format!("{output_dir}/data/{exceeded_name}");

    if render_only {
        if !std::path::Path::new(&dat_path).exists() {
            println!("  {dat_name}: no data file, skipping");
            return;
        }
    } else {
        let s0 = p0.sizes(steps);
        let s1 = p1.sizes(steps);
        let s2 = p2.sizes(steps);
        let n0 = s0.len();
        let n1 = s1.len();
        let n2 = s2.len();
        let total = n0 * n1 * n2;

        println!(
            "\n{}: {} vs {} vs {} ({} points)",
            op.name,
            p0.name(),
            p1.name(),
            p2.name(),
            total
        );

        let arity = op.params.len();
        let results = run_parallel(total, num_threads, |flat, rng| {
            let i0 = flat % n0;
            let i1 = (flat / n0) % n1;
            let i2 = flat / (n0 * n1);
            let mut param_sizes = vec![0i64; arity];
            param_sizes[sweepable[0]] = s0[i0];
            param_sizes[sweepable[1]] = s1[i1];
            param_sizes[sweepable[2]] = s2[i2];
            for &(pi, sz) in always_fixed {
                param_sizes[pi] = sz;
            }
            (i0, i1, i2, measure_op(op, &param_sizes, rng, flags))
        });

        {
            let mut f_dat = File::create(&dat_path).expect("failed to create data file");
            let mut f_exc = File::create(&exceeded_path).expect("failed to create data file");
            writeln!(
                f_dat,
                "# {}: {} vs {} vs {} (3D scatter)",
                op.name,
                p0.name(),
                p1.name(),
                p2.name()
            )
            .unwrap();
            writeln!(
                f_dat,
                "# columns: {} {} {} ns_per_cost",
                p0.name(),
                p1.name(),
                p2.name()
            )
            .unwrap();
            writeln!(
                f_exc,
                "# exceeded: {} {} {}",
                p0.name(),
                p1.name(),
                p2.name()
            )
            .unwrap();
            for (i0, i1, i2, result) in results {
                let x = s0[i0] as f64;
                let y = s1[i1] as f64;
                let z = s2[i2] as f64;
                match result {
                    MeasureResult::Success { ns_per_cost, .. } => {
                        writeln!(f_dat, "{x} {y} {z} {ns_per_cost}").unwrap();
                    }
                    MeasureResult::CostExceeded => {
                        writeln!(f_exc, "{x} {y} {z}").unwrap();
                    }
                    MeasureResult::OtherFailure => {}
                }
            }
        }
        println!("  Saved: {dat_path}");
    }

    // Read data file, compute ranges, and generate an RGB-colored file for gnuplot.
    // gnuplot's splot "lc palette" always colors by z-coordinate, so we pre-compute
    // RGB integers from ns_per_cost and use "lc rgb variable" instead.
    let content = std::fs::read_to_string(&dat_path).expect("failed to read data file");
    let rgb_name = format!(
        "{}-{}-{}-{}-3d-rgb.dat",
        op.name,
        p0.name(),
        p1.name(),
        p2.name()
    );
    let rgb_path = format!("{output_dir}/data/{rgb_name}");
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    let mut z_min = f64::INFINITY;
    let mut z_max = f64::NEG_INFINITY;
    {
        let mut f_rgb = File::create(&rgb_path).expect("failed to create RGB data file");
        writeln!(f_rgb, "# derived from {dat_name}: x y z rgb_color").unwrap();
        for line in content.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 4
                && let (Ok(x), Ok(y), Ok(z), Ok(npc)) = (
                    cols[0].parse::<f64>(),
                    cols[1].parse::<f64>(),
                    cols[2].parse::<f64>(),
                    cols[3].parse::<f64>(),
                )
            {
                let rgb = palette_to_rgb(npc);
                writeln!(f_rgb, "{x} {y} {z} {rgb}").unwrap();
                x_min = x_min.min(x);
                x_max = x_max.max(x);
                y_min = y_min.min(y);
                y_max = y_max.max(y);
                z_min = z_min.min(z);
                z_max = z_max.max(z);
            }
        }
    }

    if x_min > x_max {
        return;
    }

    let title = gnuplot_escape(&format!(
        "{} ns/cost: {} vs {} vs {}",
        op.name,
        p0.name(),
        p1.name(),
        p2.name()
    ));
    let png_name = format!(
        "{}-{}-{}-{}-3d.png",
        op.name,
        p0.name(),
        p1.name(),
        p2.name()
    );

    script.push_str(&format!(
        "\n# --- {name} 3D scatter ---\n\
         reset\n\
         set terminal pngcairo size 1200,1000\n\
         set output '{png_name}'\n\
         set title '{title}' noenhanced\n\
         set xlabel '{xlabel}' noenhanced\n\
         set ylabel '{ylabel}' noenhanced\n\
         set zlabel '{zlabel}' noenhanced rotate by 90\n\
         {preamble}\
         set xrange [{x_min}:{x_max}]\n\
         set yrange [{y_min}:{y_max}]\n\
         set zrange [{z_min}:{z_max}]\n\
         set view 50, 300\n\
         splot 'data/{rgb_name}' using 1:2:3:4 with points pt 7 ps 1 lc rgb variable notitle\n",
        name = op.name,
        xlabel = gnuplot_escape(p0.name()),
        ylabel = gnuplot_escape(p1.name()),
        zlabel = gnuplot_escape(p2.name()),
        preamble = heatmap_preamble(),
    ));
}

fn benchmark_variadic(
    op: &OpDef,
    max_args: usize,
    output_dir: &str,
    num_threads: usize,
    flags: ClvmFlags,
    render_only: bool,
    script: &mut String,
) {
    let dat_name = format!("{}-variadic.dat", op.name);
    let dat_path = format!("{output_dir}/data/{dat_name}");

    let y_max: f64 = if render_only {
        let path = std::path::Path::new(&dat_path);
        if !path.exists() {
            println!("  {dat_name}: no data file, skipping");
            return;
        }
        let content = std::fs::read_to_string(path).expect("failed to read data file");
        content
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .filter_map(|l| {
                l.split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .filter(|v| !v.is_nan())
            .fold(BENCHMARK_TIME_PER_COST, f64::max)
            * 1.05
    } else {
        let min_args = op.params.len();
        assert!(max_args >= min_args);

        let arg_counts: Vec<usize> = (min_args..=max_args).collect();
        let n = arg_counts.len();

        println!(
            "\n{}: variadic {}..{} args ({} points)",
            op.name, min_args, max_args, n
        );

        let results = run_parallel(n, num_threads, |idx, rng| {
            (idx, measure_op_variadic(op, arg_counts[idx], rng, flags))
        });

        let mut values = vec![f64::NAN; n];
        let mut exceeded = vec![false; n];
        for (idx, result) in results {
            match result {
                MeasureResult::Success { ns_per_cost, .. } => {
                    values[idx] = ns_per_cost;
                }
                MeasureResult::CostExceeded => {
                    exceeded[idx] = true;
                }
                MeasureResult::OtherFailure => {}
            }
        }

        {
            let mut f = File::create(&dat_path).expect("failed to create data file");
            writeln!(f, "# {}: variadic {}..{} args", op.name, min_args, max_args).unwrap();
            writeln!(f, "# columns: num_args ns_per_cost exceeded").unwrap();
            for i in 0..n {
                writeln!(
                    f,
                    "{} {} {}",
                    arg_counts[i],
                    if exceeded[i] {
                        "NaN".to_string()
                    } else {
                        format!("{}", values[i])
                    },
                    if exceeded[i] { 1 } else { 0 },
                )
                .unwrap();
            }
        }
        println!("  Saved: {dat_path}");

        values
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(BENCHMARK_TIME_PER_COST, f64::max)
            * 1.05
    };

    let title = gnuplot_escape(&format!("{} ns/cost vs arg count", op.name));
    let png_name = format!("{}-variadic.png", op.name);

    script.push_str(&format!(
        "\n# --- {name} variadic ---\n\
         reset\n\
         set terminal pngcairo size 1100,600\n\
         set output '{png_name}'\n\
         set title '{title}' noenhanced\n\
         set xlabel 'number of arguments'\n\
         set ylabel 'ns / cost'\n\
         set yrange [0:{y_max}]\n\
         set arrow nohead from graph 0, first {bm_time} to graph 1, first {bm_time} lc rgb '#ff0000' lw 1.0\n\
         plot 'data/{dat_name}' using 1:($3==0?$2:NaN) with points lc rgb 'blue' pt 7 ps 0.5 notitle, \\\n\
              'data/{dat_name}' using ($3==1?$1:NaN):(0) with points pt 2 lc rgb 'red' ps 1.0 notitle\n",
        name = op.name,
        bm_time = BENCHMARK_TIME_PER_COST,
    ));
}

fn main() {
    let args = Args::parse();

    if args.list {
        for op in OPERATORS {
            println!("{} (opcode {})", op.name, op.opcode);
        }
        return;
    }

    let num_threads = args.threads.max(1);
    let mut flags = ClvmFlags::empty();
    if args.new_cost_model {
        flags |= ClvmFlags::NEW_COST_MODEL;
    }
    let output_dir = args.output_dir.unwrap_or_else(|| {
        if args.new_cost_model {
            "measurements-v2".to_string()
        } else {
            "measurements".to_string()
        }
    });
    let render_only = args.render_only;
    if render_only {
        println!("Render-only mode: regenerating gnuplot scripts from existing data");
    } else {
        println!("Using {num_threads} thread(s)");
    }
    if args.new_cost_model {
        println!("Using NEW_COST_MODEL");
    }

    create_dir_all(format!("{output_dir}/data")).expect("failed to create data directory");
    create_dir_all(format!("{output_dir}/gnuplot")).expect("failed to create gnuplot directory");

    for op in OPERATORS {
        if !args.operator.is_empty() && !args.operator.iter().any(|f| f == op.name) {
            continue;
        }

        let steps = if args.hires { op.steps * 4 } else { op.steps };

        if !render_only {
            warmup_op(op, flags);
        }

        let sweepable: Vec<usize> = op
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_sweepable())
            .map(|(i, _)| i)
            .collect();
        let always_fixed: Vec<(usize, i64)> = op
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_sweepable())
            .map(|(i, p)| (i, p.fixed()))
            .collect();

        println!(
            "\n=== {} (opcode {}, {} params, {} sweepable) ===",
            op.name,
            op.opcode,
            op.params.len(),
            sweepable.len()
        );

        let mut script = String::new();

        match sweepable.len() {
            0 => {
                benchmark_histogram(
                    op,
                    steps,
                    &output_dir,
                    num_threads,
                    flags,
                    render_only,
                    &mut script,
                );
            }
            1 => {
                benchmark_1d_slice(
                    op,
                    sweepable[0],
                    &always_fixed,
                    steps,
                    &output_dir,
                    num_threads,
                    flags,
                    render_only,
                    &mut script,
                );
            }
            2 => {
                benchmark_2d(
                    op,
                    sweepable[0],
                    sweepable[1],
                    &always_fixed,
                    steps,
                    &output_dir,
                    num_threads,
                    flags,
                    render_only,
                    &mut script,
                );
                for &fsz in op.params[sweepable[1]].fixed_values() {
                    let mut fixed = always_fixed.clone();
                    fixed.push((sweepable[1], fsz));
                    benchmark_1d_slice(
                        op,
                        sweepable[0],
                        &fixed,
                        steps,
                        &output_dir,
                        num_threads,
                        flags,
                        render_only,
                        &mut script,
                    );
                }
                for &fsz in op.params[sweepable[0]].fixed_values() {
                    let mut fixed = always_fixed.clone();
                    fixed.push((sweepable[0], fsz));
                    benchmark_1d_slice(
                        op,
                        sweepable[1],
                        &fixed,
                        steps,
                        &output_dir,
                        num_threads,
                        flags,
                        render_only,
                        &mut script,
                    );
                }
            }
            3 => {
                benchmark_3d(
                    op,
                    &sweepable,
                    &always_fixed,
                    steps,
                    &output_dir,
                    num_threads,
                    flags,
                    render_only,
                    &mut script,
                );
                for &fix in &sweepable {
                    let varying: Vec<usize> =
                        sweepable.iter().copied().filter(|&i| i != fix).collect();
                    for &fsz in op.params[fix].fixed_values() {
                        let mut fixed = always_fixed.clone();
                        fixed.push((fix, fsz));
                        benchmark_2d(
                            op,
                            varying[0],
                            varying[1],
                            &fixed,
                            steps,
                            &output_dir,
                            num_threads,
                            flags,
                            render_only,
                            &mut script,
                        );
                    }
                }
            }
            n => {
                eprintln!("  unsupported number of sweepable params: {n}, skipping");
            }
        }

        if op.variadic > 0 {
            benchmark_variadic(
                op,
                op.variadic,
                &output_dir,
                num_threads,
                flags,
                render_only,
                &mut script,
            );
        }

        if !script.is_empty() {
            let fragment_path = format!("{output_dir}/gnuplot/{}.gnuplot", op.name);
            std::fs::write(&fragment_path, &script).expect("failed to write gnuplot fragment");
            println!("  Script: {fragment_path}");
        }
    }

    // Assemble render.gnuplot from all per-operator fragments in the gnuplot/ subdirectory.
    let gnuplot_dir = format!("{output_dir}/gnuplot");
    let mut fragments: Vec<String> = std::fs::read_dir(&gnuplot_dir)
        .expect("failed to read gnuplot directory")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".gnuplot") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    fragments.sort();

    let mut render = String::from(
        "# Auto-generated gnuplot script. Run with:\n\
         #   (cd <output_dir> && gnuplot render.gnuplot)\n\
         # or run individual operator scripts directly.\n\n",
    );
    for frag in &fragments {
        render.push_str(&format!("load 'gnuplot/{frag}'\n"));
    }

    let script_path = format!("{output_dir}/render.gnuplot");
    std::fs::write(&script_path, &render).expect("failed to write gnuplot script");
    println!("\nGnuplot script saved: {script_path}");
    println!(
        "Includes {} operator(s): {}",
        fragments.len(),
        fragments.join(", ")
    );
    println!("Run: (cd {output_dir} && gnuplot render.gnuplot)");
}
