use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::ChiaDialect;
use clvmr::run_program::run_program;
use linreg::linear_regression_of;
use std::fs::{create_dir_all, File};
use std::io::{sink, Write};
use std::time::Instant;

#[derive(Clone, Copy)]
enum Placeholder {
    SingleArg(Option<NodePtr>),
    TwoArgs(Option<NodePtr>, Option<NodePtr>),
    ThreeArgs(Option<NodePtr>, Option<NodePtr>, Option<NodePtr>),
}

#[derive(Clone, Copy)]
enum OpArgs {
    SingleArg(NodePtr),
    TwoArgs(NodePtr, NodePtr),
    ThreeArgs(NodePtr, NodePtr, NodePtr),
}

// builds calls in the form:
// (<op> arg arg ...)
// where "num" specifies the number of arguments
// if arg is a pair, it's unwrapped into two arguments
fn build_call(
    a: &mut Allocator,
    op: u32,
    arg: OpArgs,
    num: i32,
    extra: Option<NodePtr>,
) -> NodePtr {
    let mut args = a.nil();
    for _i in 0..num {
        match arg {
            OpArgs::SingleArg(a1) => {
                args = a.new_pair(a1, args).unwrap();
            }
            OpArgs::TwoArgs(first, second) => {
                args = a.new_pair(second, args).unwrap();
                args = a.new_pair(first, args).unwrap();
            }
            OpArgs::ThreeArgs(first, second, third) => {
                args = a.new_pair(third, args).unwrap();
                args = a.new_pair(second, args).unwrap();
                args = a.new_pair(first, args).unwrap();
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
    mut arg: OpArgs,
    num: i32,
    extra: Option<NodePtr>,
) -> NodePtr {
    let op_code = a.new_number(op.into()).unwrap();
    for _i in 0..num {
        let mut args = a.nil();
        match arg {
            OpArgs::SingleArg(a1) => {
                args = a.new_pair(a1, args).unwrap();
            }
            OpArgs::TwoArgs(first, second) => {
                args = a.new_pair(second, args).unwrap();
                args = a.new_pair(first, args).unwrap();
            }
            OpArgs::ThreeArgs(first, second, third) => {
                args = a.new_pair(third, args).unwrap();
                args = a.new_pair(second, args).unwrap();
                args = a.new_pair(first, args).unwrap();
            }
        }
        if let Some(extra) = extra {
            args = a.new_pair(extra, args).unwrap();
        }
        args = a.new_pair(op_code, args).unwrap();
        arg = OpArgs::SingleArg(args);
    }
    match arg {
        OpArgs::SingleArg(ret) => ret,
        _ => {
            panic!("unexpected");
        }
    }
}

fn quote(a: &mut Allocator, v: NodePtr) -> NodePtr {
    a.new_pair(a.one(), v).unwrap()
}

fn subst(arg: Option<NodePtr>, substitution: NodePtr) -> NodePtr {
    arg.unwrap_or(substitution)
}

fn substitute(args: Placeholder, s: NodePtr) -> OpArgs {
    match args {
        Placeholder::SingleArg(n) => OpArgs::SingleArg(subst(n, s)),
        Placeholder::TwoArgs(n0, n1) => OpArgs::TwoArgs(subst(n0, s), subst(n1, s)),
        Placeholder::ThreeArgs(n0, n1, n2) => {
            OpArgs::ThreeArgs(subst(n0, s), subst(n1, s), subst(n2, s))
        }
    }
}

fn time_invocation(a: &mut Allocator, op: u32, arg: OpArgs, flags: u32) -> f64 {
    let call = build_call(a, op, arg, 1, None);
    //println!("{:x?}", &Node::new(a, call));
    let dialect = ChiaDialect::new(0x0200);
    let start = Instant::now();
    let r = run_program(a, &dialect, call, a.nil(), 11000000000);
    if (flags & ALLOW_FAILURE) == 0 {
        r.unwrap();
    }
    if (flags & EXPONENTIAL_COST) != 0 {
        (start.elapsed().as_nanos() as f64).sqrt()
    } else {
        start.elapsed().as_nanos() as f64
    }
}

// returns the time per byte
// measures run-time of many calls
fn time_per_byte(a: &mut Allocator, op: &Operator, output: &mut dyn Write) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let mut atom = vec![0; 10000000];
    for (i, value) in atom.iter_mut().enumerate() {
        *value = (i + 1) as u8;
    }
    for _k in 0..3 {
        for i in 1..1000 {
            let scale = if (op.flags & LARGE_BUFFERS) != 0 {
                1000
            } else {
                1
            };

            let subst = a.new_atom(&atom[0..(i * scale)]).unwrap();
            let arg = substitute(op.arg, quote(a, subst));
            let sample = (
                i as f64 * scale as f64,
                time_invocation(a, op.opcode, arg, op.flags),
            );
            writeln!(output, "{}\t{}", sample.0, sample.1).expect("failed to write");
            samples.push(sample);
            a.restore_checkpoint(&checkpoint);
        }
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// returns the time per argument
// measures the run-time of many calls with varying number of arguments, to
// establish how much time each additional argument contributes
fn time_per_arg(a: &mut Allocator, op: &Operator, output: &mut dyn Write) -> f64 {
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(0);

    let subst = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let arg = substitute(op.arg, quote(a, subst));

    let checkpoint = a.checkpoint();

    for _k in 0..3 {
        for i in (0..1000).step_by(5) {
            let call = build_call(a, op.opcode, arg, i, op.extra);
            let start = Instant::now();
            let r = run_program(a, &dialect, call, a.nil(), 11000000000);
            if (op.flags & ALLOW_FAILURE) == 0 {
                r.unwrap();
            }
            let duration = start.elapsed();
            let sample = (i as f64, duration.as_nanos() as f64);
            writeln!(output, "{}\t{}", sample.0, sample.1).expect("failed to write");
            samples.push(sample);

            a.restore_checkpoint(&checkpoint);
        }
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// measure run-time of many *nested* calls, to establish how much longer it
// takes, approximately, for each additional nesting.
fn base_call_time(a: &mut Allocator, op: &Operator, output: &mut dyn Write) -> f64 {
    let mut samples = Vec::<(f64, f64)>::new();
    let dialect = ChiaDialect::new(0);

    let subst = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let arg = substitute(op.arg, quote(a, subst));

    let checkpoint = a.checkpoint();

    for _k in 0..3 {
        for i in 1..100 {
            a.restore_checkpoint(&checkpoint);
            let call = build_nested_call(a, op.opcode, arg, i, op.extra);
            let start = Instant::now();
            let r = run_program(a, &dialect, call, a.nil(), 11_000_000_000);
            if (op.flags & ALLOW_FAILURE) == 0 {
                r.unwrap();
            }
            let duration_ns = start.elapsed().as_nanos() as f64;
            writeln!(output, "{}\t{}", i, duration_ns).expect("failed to write");
            samples.push((i as f64, duration_ns));
        }
    }

    // duration = base_cost (intercept) + slope (nested_cost) * i
    let (_slope, intercept): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");

    // 'intercept' is the estimated base cost per call (in ns)
    intercept.max(100.0)
}

fn base_call_time_no_nest(a: &mut Allocator, op: &Operator, per_arg_time: f64) -> f64 {
    let mut total_time: f64 = 0.0;
    let mut num_samples = 0;

    let subst = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let arg = substitute(op.arg, quote(a, subst));

    let checkpoint = a.checkpoint();

    for _i in 0..300 {
        a.restore_checkpoint(&checkpoint);
        total_time += time_invocation(a, op.opcode, arg, op.flags & !EXPONENTIAL_COST);
        num_samples += 1;
    }

    ((total_time - per_arg_time * num_samples as f64) / num_samples as f64).max(100.0)
}

const PER_BYTE_COST: u32 = 1;
const PER_ARG_COST: u32 = 2;
const NESTING_BASE_COST: u32 = 4;
const EXPONENTIAL_COST: u32 = 8;
const LARGE_BUFFERS: u32 = 16;
const ALLOW_FAILURE: u32 = 32;

struct Operator {
    opcode: u32,
    name: &'static str,
    arg: Placeholder,
    extra: Option<NodePtr>,
    flags: u32,
}

/// Measure CPU cost of CLVM operators to aid in determining their cost
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// enable plotting of measurements
    #[arg(short, long, default_value_t = false)]
    plot: bool,
}

fn maybe_open(plot: bool, op: &str, name: &str) -> Box<dyn Write> {
    if plot {
        create_dir_all("measurements").expect("failed to create directory");
        Box::new(File::create(format!("measurements/{op}-{name}")).expect("failed to open file"))
    } else {
        Box::new(sink())
    }
}

fn write_gnuplot_header(gnuplot: &mut dyn Write, op: &Operator, out: &str, xlabel: &str) {
    writeln!(
        gnuplot,
        "set output \"{}-{out}.png\"
set title \"{}\"
set xlabel \"{xlabel}\"
set ylabel \"nanoseconds{}\"",
        op.name,
        op.name,
        if (op.flags & EXPONENTIAL_COST) != 0 {
            " log"
        } else {
            ""
        }
    )
    .expect("failed to write");
}

fn print_plot(gnuplot: &mut dyn Write, a: &f64, b: &f64, op: &str, name: &str) {
    writeln!(gnuplot, "f(x) = {a}*x+{b}").expect("failed to write");
    writeln!(
        gnuplot,
        "plot \"{op}-{name}.log\" using 1:2 with dots title \"measured\", f(x) title \"fitting\""
    )
    .expect("failed to write");
}

pub fn main() {
    let options = Args::parse();

    let mut a = Allocator::new();

    let g1 = a.new_atom(&hex::decode("97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb").unwrap()).unwrap();
    let g2 = a.new_atom(&hex::decode("93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8").unwrap()).unwrap();

    let g1 = quote(&mut a, g1);
    let g2 = quote(&mut a, g2);

    // for secp256k1_verify
    let k1_pk = a
        .new_atom(
            &hex::decode("02888b0c110ef0b4962e3fc6929cbba7a8bb25b4b2c885f55c76365018c909b439")
                .unwrap(),
        )
        .unwrap();
    let k1_pk = quote(&mut a, k1_pk);
    let k1_msg = a
        .new_atom(
            &hex::decode("74c2941eb2ebe5aa4f2287a4c5e506a6290c045004058de97a7edf0122548668")
                .unwrap(),
        )
        .unwrap();
    let k1_msg = quote(&mut a, k1_msg);
    let k1_sig = a.new_atom(&hex::decode("1acb7a6e062e78ccd4237b12c22f02b5a8d9b33cb3ba13c35e88e036baa1cbca75253bb9a96ffc48b43196c69c2972d8f965b1baa4e52348d8081cde65e6c018").unwrap()).unwrap();
    let k1_sig = quote(&mut a, k1_sig);

    // for secp256r1_verify
    let r1_pk = a.new_atom(&hex::decode("0437a1674f3883b7171a11a20140eee014947b433723cf9f181a18fee4fcf96056103b3ff2318f00cca605e6f361d18ff0d2d6b817b1fa587e414f8bb1ab60d2b9").unwrap()).unwrap();
    let r1_pk = quote(&mut a, r1_pk);
    let r1_msg = a
        .new_atom(
            &hex::decode("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08")
                .unwrap(),
        )
        .unwrap();
    let r1_msg = quote(&mut a, r1_msg);
    let r1_sig = a.new_atom(&hex::decode("e8de121f4cceca12d97527cc957cca64a4bcfc685cffdee051b38ee81cb22d7e2c187fec82c731018ed2d56f08a4a5cbc40c5bfe9ae18c02295bb65e7f605ffc").unwrap()).unwrap();
    let r1_sig = quote(&mut a, r1_sig);

    let number = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let number = quote(&mut a, number);

    let ops: [Operator; 20] = [
        Operator {
            opcode: 60,
            name: "modpow (modulus cost)",
            arg: Placeholder::ThreeArgs(Some(number), Some(number), None),
            extra: None,
            flags: PER_BYTE_COST | EXPONENTIAL_COST,
        },
        Operator {
            opcode: 60,
            name: "modpow (exponent cost)",
            arg: Placeholder::ThreeArgs(Some(number), None, Some(number)),
            extra: None,
            flags: PER_BYTE_COST | EXPONENTIAL_COST,
        },
        Operator {
            opcode: 60,
            name: "modpow (value cost)",
            arg: Placeholder::ThreeArgs(None, Some(number), Some(number)),
            extra: None,
            flags: PER_BYTE_COST,
        },
        Operator {
            opcode: 29,
            name: "point_add",
            arg: Placeholder::SingleArg(Some(g1)),
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST,
        },
        Operator {
            opcode: 49,
            name: "g1_subtract",
            arg: Placeholder::SingleArg(Some(g1)),
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST,
        },
        Operator {
            opcode: 50,
            name: "g1_multiply",
            arg: Placeholder::TwoArgs(Some(g1), None),
            extra: Some(g1),
            flags: PER_BYTE_COST,
        },
        Operator {
            opcode: 51,
            name: "g1_negate",
            arg: Placeholder::SingleArg(Some(g1)),
            extra: None,
            flags: 0,
        },
        Operator {
            opcode: 52,
            name: "g2_add",
            arg: Placeholder::SingleArg(Some(g2)),
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST,
        },
        Operator {
            opcode: 53,
            name: "g2_subtract",
            arg: Placeholder::SingleArg(Some(g2)),
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST,
        },
        Operator {
            opcode: 54,
            name: "g2_multiply",
            arg: Placeholder::TwoArgs(Some(g2), None),
            extra: Some(g2),
            flags: PER_BYTE_COST,
        },
        Operator {
            opcode: 55,
            name: "g2_negate",
            arg: Placeholder::SingleArg(Some(g2)),
            extra: None,
            flags: 0,
        },
        Operator {
            opcode: 56,
            name: "g1_map",
            arg: Placeholder::SingleArg(None),
            extra: None,
            flags: PER_BYTE_COST | LARGE_BUFFERS,
        },
        Operator {
            opcode: 57,
            name: "g2_map",
            arg: Placeholder::SingleArg(None),
            extra: None,
            flags: PER_BYTE_COST | LARGE_BUFFERS,
        },
        Operator {
            opcode: 58,
            name: "bls_pairing_identity",
            arg: Placeholder::TwoArgs(Some(g1), Some(g2)),
            extra: None,
            flags: PER_ARG_COST | ALLOW_FAILURE,
        },
        Operator {
            opcode: 59,
            name: "bls_verify",
            arg: Placeholder::TwoArgs(Some(g1), Some(g2)),
            extra: Some(g2),
            flags: PER_ARG_COST | ALLOW_FAILURE,
        },
        Operator {
            opcode: 0x13d61f00,
            name: "secp256k1_verify",
            arg: Placeholder::ThreeArgs(Some(k1_pk), Some(k1_msg), Some(k1_sig)),
            extra: None,
            flags: ALLOW_FAILURE,
        },
        Operator {
            opcode: 0x1c3a8f00,
            name: "secp256r1_verify",
            arg: Placeholder::ThreeArgs(Some(r1_pk), Some(r1_msg), Some(r1_sig)),
            extra: None,
            flags: ALLOW_FAILURE,
        },
        Operator {
            opcode: 11,
            name: "sha256",
            arg: Placeholder::SingleArg(Some(g1)),
            extra: None,
            flags: NESTING_BASE_COST | PER_ARG_COST | PER_BYTE_COST | LARGE_BUFFERS,
        },
        Operator {
            opcode: 62,
            name: "keccak256",
            arg: Placeholder::SingleArg(Some(g1)),
            extra: None,
            flags: NESTING_BASE_COST | PER_ARG_COST | PER_BYTE_COST | LARGE_BUFFERS,
        },
        Operator {
            opcode: 65,
            name: "sha256tree (atom)",
            arg: Placeholder::SingleArg(None),
            extra: None,
            flags: NESTING_BASE_COST | PER_ARG_COST | PER_BYTE_COST | LARGE_BUFFERS,
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

    let mut gnuplot = maybe_open(options.plot, "gen", "graphs.gnuplot");
    writeln!(gnuplot, "set term png size 1200,600").expect("failed to write");
    writeln!(gnuplot, "set key top right").expect("failed to write");

    for op in &ops {
        println!("opcode: {} ({})", op.name, op.opcode);
        let time_per_byte = if (op.flags & PER_BYTE_COST) != 0 {
            let mut output = maybe_open(options.plot, op.name, "per-byte.log");
            let time_per_byte = time_per_byte(&mut a, op, &mut *output);
            println!("   time: per-byte: {time_per_byte:.2}ns");
            println!("   cost: per-byte: {:.0}", time_per_byte * cost_scale);
            time_per_byte
        } else {
            0.0
        };
        let time_per_arg = if (op.flags & PER_ARG_COST) != 0 {
            let mut output = maybe_open(options.plot, op.name, "per-arg.log");
            let time_per_arg = time_per_arg(&mut a, op, &mut *output);
            println!("   time: per-arg: {time_per_arg:.2}ns");
            println!("   cost: per-arg: {:.0}", time_per_arg * arg_cost_scale);
            time_per_arg
        } else {
            0.0
        };
        let base_call_time = if (op.flags & NESTING_BASE_COST) != 0 {
            let mut output = maybe_open(options.plot, op.name, "base.log");
            write_gnuplot_header(&mut *gnuplot, op, "base", "num nested calls");
            let base_call_time = base_call_time(&mut a, op, &mut *output);
            println!("   time: base: {base_call_time:.2}ns");
            println!("   cost: base: {:.0}", base_call_time * base_cost_scale);

            print_plot(&mut *gnuplot, &base_call_time, &0.0, op.name, "base");
            base_call_time
        } else {
            let base_call_time = base_call_time_no_nest(&mut a, op, time_per_arg);
            println!("   time: base: {base_call_time:.2}ns");
            println!("   cost: base: {:.0}", base_call_time * base_cost_scale);
            base_call_time
        };

        // we adjust the base_Call_time here to make the curve fitting match
        let base_call_time = if (op.flags & EXPONENTIAL_COST) != 0 {
            base_call_time.sqrt()
        } else {
            base_call_time
        };
        if (op.flags & PER_ARG_COST) != 0 {
            write_gnuplot_header(&mut *gnuplot, op, "per-arg", "num arguments");
            print_plot(
                &mut *gnuplot,
                &time_per_arg,
                &base_call_time,
                op.name,
                "per-arg",
            );
        } else if (op.flags & PER_BYTE_COST) != 0 {
            write_gnuplot_header(&mut *gnuplot, op, "per-byte", "num bytes");
            print_plot(
                &mut *gnuplot,
                &time_per_byte,
                &base_call_time,
                op.name,
                "per-byte",
            );
        }
    }
    if options.plot {
        println!("To generate plots, run:\n   (cd measurements; gnuplot gen-graphs.gnuplot)");
    }
}
