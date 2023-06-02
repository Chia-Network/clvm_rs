use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::chia_dialect::{ChiaDialect, ENABLE_BLS_OPS_OUTSIDE_GUARD};
use clvmr::run_program::run_program;
use linreg::linear_regression_of;
use std::time::Instant;

// builds calls in the form:
// (<op> arg arg ...)
// where "num" specifies the number of arguments
// if arg is a pair, it's unwrapped into two arguments
fn build_call(
    a: &mut Allocator,
    op: u32,
    arg: NodePtr,
    num: i32,
    extra: Option<NodePtr>,
) -> NodePtr {
    let mut args = a.null();
    for _i in 0..num {
        match a.sexp(arg) {
            SExp::Pair(first, second) => {
                if first == a.one() {
                    args = a.new_pair(arg, args).unwrap();
                } else {
                    args = a.new_pair(second, args).unwrap();
                    args = a.new_pair(first, args).unwrap();
                }
            }
            _ => {
                panic!("unexpected");
            }
        }
    }
    if let Some(extra) = extra {
        args = a.new_pair(extra, args).unwrap();
    }
    let op_code = a.new_number(op.into()).unwrap();
    a.new_pair(op_code, args).unwrap()
}

// builds calls in the form:
// (<op> [extra] (<op> [extra] (<op> [extra] arg)))
// "extra" is optional, "num" specifies the nesting depth
fn build_nested_call(
    a: &mut Allocator,
    op: u32,
    mut arg: NodePtr,
    num: i32,
    extra: Option<NodePtr>,
) -> NodePtr {
    let op_code = a.new_number(op.into()).unwrap();
    for _i in 0..num {
        let mut args = a.null();
        args = a.new_pair(arg, args).unwrap();
        if let Some(extra) = extra {
            args = a.new_pair(extra, args).unwrap();
        }
        args = a.new_pair(op_code, args).unwrap();
        arg = args;
    }
    arg
}

fn quote(a: &mut Allocator, v: NodePtr) -> NodePtr {
    a.new_pair(a.one(), v).unwrap()
}

// returns the time per byte
// measures run-time of many calls
fn time_per_byte(a: &mut Allocator, op: u32, extra: Option<NodePtr>) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    let atom = vec![0; 10000000];
    for i in (0..10000000).step_by(1000) {
        let mut args = a.null();
        let arg = a.new_atom(&atom[0..i]).unwrap();
        let arg = quote(a, arg);
        args = a.new_pair(arg, args).unwrap();

        if let Some(extra) = extra {
            args = a.new_pair(extra, args).unwrap();
        }

        let op_code = a.new_number(op.into()).unwrap();
        let call = a.new_pair(op_code, args).unwrap();
        let start = Instant::now();
        run_program(a, &dialect, call, a.null(), 11000000000).unwrap();
        let duration = start.elapsed();
        samples.push((i as f64, duration.as_nanos() as f64));
        a.restore_checkpoint(&checkpoint);
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// returns the time per argument
// measures the run-time of many calls with varying number of arguments, to
// establish how much time each additional argument contributes
fn time_per_arg(a: &mut Allocator, op: u32, arg: NodePtr, extra: Option<NodePtr>) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    for _k in 0..3 {
        for i in 0..100 {
            let call = build_call(a, op, arg, i, extra);
            let start = Instant::now();
            run_program(a, &dialect, call, a.null(), 11000000000).unwrap();
            let duration = start.elapsed();
            samples.push((i as f64, duration.as_nanos() as f64));

            a.restore_checkpoint(&checkpoint);
        }
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// measure run-time of many *nested* calls, to establish how much longer it
// takes, approximately, for each additional nesting. The per_arg_time is
// subtracted to get the base cost
fn base_call_time(
    a: &mut Allocator,
    op: u32,
    per_arg_time: f64,
    arg: NodePtr,
    extra: Option<NodePtr>,
) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    for _k in 0..3 {
        for i in 1..100 {
            a.restore_checkpoint(&checkpoint);
            let call = build_nested_call(a, op, arg, i, extra);
            let start = Instant::now();
            run_program(a, &dialect, call, a.null(), 11000000000).unwrap();
            let duration = start.elapsed();
            let duration = (duration.as_nanos() as f64) - (per_arg_time * i as f64);
            samples.push((i as f64, duration));

            a.restore_checkpoint(&checkpoint);
        }
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

fn base_call_time_no_nest(
    a: &mut Allocator,
    op: u32,
    arg: NodePtr,
    per_arg_time: f64,
    extra: Option<NodePtr>,
) -> f64 {
    let checkpoint = a.checkpoint();
    let dialect = ChiaDialect::new(ENABLE_BLS_OPS_OUTSIDE_GUARD);

    let mut total_time: u64 = 0;
    let mut num_samples = 0;

    for _k in 0..3 {
        for _i in 1..100 {
            a.restore_checkpoint(&checkpoint);
            let call = build_call(a, op, arg, 1, extra);
            let start = Instant::now();
            run_program(a, &dialect, call, a.null(), 11000000000).unwrap();
            let duration = start.elapsed();
            total_time += duration.as_nanos() as u64;
            num_samples += 1;

            a.restore_checkpoint(&checkpoint);
        }
    }

    (total_time as f64 - per_arg_time * num_samples as f64) / num_samples as f64
}

enum Mode {
    Nesting,
    Unary,
    FreeBytes,
    MultiArg,
}

struct Operator {
    opcode: u32,
    name: &'static str,
    arg: NodePtr,
    extra: Option<NodePtr>,
    mode: Mode,
}

pub fn main() {
    let mut a = Allocator::new();

    let g1 = a.new_atom(&hex::decode("97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb").unwrap()).unwrap();
    let g2 = a.new_atom(&hex::decode("93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8").unwrap()).unwrap();

    let g1 = quote(&mut a, g1);
    let g2 = quote(&mut a, g2);
    let g1_g2_pair = a.new_pair(g1, g2).unwrap();

    let ops: [Operator; 12] = [
        Operator {
            opcode: 29,
            name: "point_add",
            arg: g1,
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 49,
            name: "g1_subtract",
            arg: g1,
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 50,
            name: "g1_multiply",
            arg: g1,
            extra: Some(g1),
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 51,
            name: "g1_negate",
            arg: g1,
            extra: None,
            mode: Mode::Unary,
        },
        Operator {
            opcode: 52,
            name: "g2_add",
            arg: g2,
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 53,
            name: "g2_subtract",
            arg: g2,
            extra: None,
            mode: Mode::Nesting,
        },
        Operator {
            opcode: 54,
            name: "g2_multiply",
            arg: g2,
            extra: Some(g2),
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 55,
            name: "g2_negate",
            arg: g2,
            extra: None,
            mode: Mode::Unary,
        },
        Operator {
            opcode: 56,
            name: "g1_map",
            arg: 0,
            extra: None,
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 57,
            name: "g2_map",
            arg: 0,
            extra: None,
            mode: Mode::FreeBytes,
        },
        Operator {
            opcode: 58,
            name: "bls_pairing_identity",
            arg: g1_g2_pair,
            extra: None,
            mode: Mode::MultiArg,
        },
        Operator {
            opcode: 59,
            name: "bls_verify",
            arg: g1_g2_pair,
            extra: Some(g2),
            mode: Mode::MultiArg,
        },
    ];

    // this "magic" scaling depends on the computer you run the tests on.
    // It's calibrated against the timing of point_add, which has a cost
    let cost_scale = ((101094.0 / 39000.0) + (1343980.0 / 131000.0)) / 2.0;
    let base_cost_scale = 101094.0 / 42500.0;
    let arg_cost_scale = 1343980.0 / 129000.0;
    println!("cost scale: {cost_scale}");
    println!("base cost scale: {base_cost_scale}");
    println!("arg cost scale: {arg_cost_scale}");

    for op in &ops {
        println!("opcode: {} ({})", op.name, op.opcode);
        match op.mode {
            Mode::Nesting => {
                let time_per_arg = time_per_arg(&mut a, op.opcode, op.arg, op.extra);
                let base_call_time =
                    base_call_time(&mut a, op.opcode, time_per_arg, op.arg, op.extra);
                println!("   time: base: {base_call_time:.2}ns per-arg: {time_per_arg:.2}ns");
                println!(
                    "   cost: base: {:.0} per-arg: {:.0}",
                    base_call_time * base_cost_scale,
                    time_per_arg * arg_cost_scale
                );
            }
            Mode::Unary => {
                let base_call_time = base_call_time(&mut a, op.opcode, 0.0, op.arg, op.extra);
                println!("   time: base: {base_call_time:.2}ns");
                println!("   cost: base: {:.0}", base_call_time * cost_scale);
            }
            Mode::FreeBytes => {
                let time_per_byte = time_per_byte(&mut a, op.opcode, Some(op.arg));
                let base_call_time = base_call_time(&mut a, op.opcode, 0.0, g1, op.extra);
                println!("   time: base: {base_call_time:.2}ns per-byte: {time_per_byte:.2}ns");
                println!(
                    "   cost: base: {:.0} per-byte: {:.0}",
                    base_call_time * base_cost_scale,
                    time_per_byte * cost_scale
                );
            }
            Mode::MultiArg => {
                let time_per_arg = time_per_arg(&mut a, op.opcode, op.arg, op.extra);
                let base_call_time =
                    base_call_time_no_nest(&mut a, op.opcode, op.arg, time_per_arg, op.extra);
                println!("   time: base: {base_call_time:.2}ns per-arg: {time_per_arg:.2}ns");
                println!(
                    "   cost: base: {:.0} per-arg: {:.0}",
                    base_call_time * cost_scale,
                    time_per_arg * cost_scale
                );
            }
        }
    }
}
