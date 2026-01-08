use clap::Parser;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::ChiaDialect;
use clvmr::run_program::run_program;
use linreg::linear_regression_of;
use rand::{RngCore, SeedableRng, rngs::StdRng};
use std::fs::{File, create_dir_all};
use std::io::{Write, sink};
use std::time::Instant;

const DIALECT_FLAGS: u32 = 0;

// When specifying the signature of operators, some arguments may be fixed
// constants. The None argument slots will be replaced by the benchmark for the
// various invocations of the operator.
#[derive(Clone, Copy)]
enum Placeholder {
    SingleArg(Option<NodePtr>),
    TwoArgs(Option<NodePtr>, Option<NodePtr>),
    ThreeArgs(Option<NodePtr>, Option<NodePtr>, Option<NodePtr>),
}

// This enum is used as the concrete arguments to a call, after substitution
#[derive(Clone, Copy)]
enum OpArgs {
    SingleArg(NodePtr),
    TwoArgs(NodePtr, NodePtr),
    ThreeArgs(NodePtr, NodePtr, NodePtr),
}

struct Average {
    sum: f64,
    num_samples: u64,
}

impl Average {
    pub fn new() -> Average {
        Self {
            sum: 0.0,
            num_samples: 0,
        }
    }
    pub fn add(&mut self, val: f64) {
        self.sum += val;
        self.num_samples += 1;
    }

    pub fn compute(&self) -> f64 {
        self.sum / self.num_samples as f64
    }
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

fn subst(arg: Option<NodePtr>, substitution: &mut impl FnMut() -> NodePtr) -> NodePtr {
    match arg {
        Some(n) => n,
        None => substitution(),
    }
}

fn substitute(args: Placeholder, s: &mut impl FnMut() -> NodePtr) -> OpArgs {
    match args {
        Placeholder::SingleArg(n) => OpArgs::SingleArg(subst(n, s)),
        Placeholder::TwoArgs(n0, n1) => OpArgs::TwoArgs(subst(n0, s), subst(n1, s)),
        Placeholder::ThreeArgs(n0, n1, n2) => {
            OpArgs::ThreeArgs(subst(n0, s), subst(n1, s), subst(n2, s))
        }
    }
}

// returns time measurements in nanoseconds (but maybe adjusted for exponential growth)
// raw time measurement in nanoseconds
// CLVM cost of operator
fn time_invocation(a: &mut Allocator, call: NodePtr, op: &Operator) -> (f64, f64, u64) {
    //println!("{:x?}", &Node::new(a, call));
    let dialect = ChiaDialect::new(DIALECT_FLAGS);
    let start = Instant::now();
    let r = run_program(a, &dialect, call, a.nil(), 11_000_000_000);
    let cost = if (op.flags & ALLOW_FAILURE) == 0 {
        r.expect("operator failed").0
    } else {
        assert!(
            (op.flags & PLOT_COST) == 0,
            "PLOT_COST cannot be combined with ALLOW_FAILURE. The cost of an operator is unknown if it fails"
        );
        0
    };
    let duration = start.elapsed().as_nanos() as f64;
    if op.root != 1 {
        (duration.powf(1.0 / (op.root as f64)), duration, cost)
    } else {
        (duration, duration, cost)
    }
}

// returns the time per byte
// measures run-time of many calls with the variable argument substituted for
// buffers of variying sizes
fn time_per_byte(a: &mut Allocator, op: &Operator, output: &mut dyn Write) -> f64 {
    let checkpoint = a.checkpoint();
    let mut samples = Vec::<(f64, f64)>::new();
    let max_atom_size = 1000 * op.arg_scale;
    let mut atom = vec![0; max_atom_size * 3];
    let mut rng = StdRng::seed_from_u64(0x1337);

    if (op.flags & MANY_ONES_ARG) != 0 {
        atom.fill(0xff);
        // we want the arguments to be different
        atom[max_atom_size - 1] = 1;
        atom[2 * max_atom_size - 1] = 2;
        atom[3 * max_atom_size - 1] = 3;
    } else if (op.flags & MANY_ZERO_ARG) != 0 {
        // values have few 1-bits
        atom.fill(0x0);
        atom[0] = 0x40;
        atom[max_atom_size] = 0x40;
        atom[2 * max_atom_size] = 0x40;
        // we want the arguments to be different
        atom[max_atom_size - 1] = 1;
        atom[2 * max_atom_size - 1] = 2;
        atom[3 * max_atom_size - 1] = 3;
    } else {
        rng.fill_bytes(atom.as_mut_slice());
    }
    if (op.flags & POSITIVE_ARGS) != 0 {
        atom[0] &= 0x7f;
        atom[max_atom_size] &= 0x7f;
        atom[2 * max_atom_size] &= 0x7f;
    }

    let reps = if (op.flags & LIMIT_REPS) == 0 { 3 } else { 1 };
    let mut avg_factor = Average::new();
    for _k in 0..reps {
        for i in 1..1000 {
            let mut idx = 0;
            let arg = substitute(op.arg, &mut || {
                let atom = a
                    .new_atom(&atom[idx * max_atom_size..][0..(i * op.arg_scale)])
                    .expect("new_atom");
                idx += 1;
                quote(a, atom)
            });
            let call = build_call(a, op.opcode, arg, 1, None);
            let (time, raw_time, cost) = time_invocation(a, call, op);
            avg_factor.add(cost as f64 / raw_time);
            let sample = (i as f64 * op.arg_scale as f64, time);
            writeln!(output, "{}\t{time}\t{raw_time}\t{cost}", sample.0).expect("failed to write");
            samples.push(sample);
            a.restore_checkpoint(&checkpoint);
        }
    }

    if (op.flags & PLOT_COST) != 0 {
        println!("   (per-byte) cost/ns: {}", avg_factor.compute());
    }
    // create a strong bias for 0 bytes to have 0 cost. Otherwise, noise may
    // cause the offset to be increased, rather than the slope, of the fitted
    // curve. But we only use the slope, as a way to reduce noise.
    for _ in 0..3000 {
        samples.push((0.0, 0.0));
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// returns the time per argument
// measures the run-time of many calls with varying number of arguments, to
// establish how much time each additional argument contributes
fn time_per_arg(a: &mut Allocator, op: &Operator, output: &mut dyn Write) -> f64 {
    let mut samples = Vec::<(f64, f64)>::new();

    let subst = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let arg = substitute(op.arg, &mut || quote(a, subst));

    let checkpoint = a.checkpoint();

    let reps = if (op.flags & LIMIT_REPS) == 0 { 3 } else { 1 };
    let mut avg_factor = Average::new();
    for _k in 0..reps {
        for i in (0..1000).step_by(5) {
            let call = build_call(a, op.opcode, arg, i, op.extra);
            let (time, raw_time, cost) = time_invocation(a, call, op);
            avg_factor.add(cost as f64 / raw_time);
            let sample = (i as f64, time);
            writeln!(output, "{}\t{time}\t{raw_time}\t{cost}", sample.0).expect("failed to write");
            samples.push(sample);
            a.restore_checkpoint(&checkpoint);
        }
    }

    if (op.flags & PLOT_COST) != 0 {
        println!("   (per-arg) cost/ns: {}", avg_factor.compute());
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

// measure run-time of many *nested* calls, to establish how much longer it
// takes for each additional nested call. The per_arg_time is subtracted to get
// the base cost. This only works for operators that can take their return
// value as an argument
fn base_call_time(
    a: &mut Allocator,
    op: &Operator,
    per_arg_time: f64,
    output: &mut dyn Write,
) -> f64 {
    let mut samples = Vec::<(f64, f64)>::new();
    let subst = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let arg = substitute(op.arg, &mut || quote(a, subst));

    let checkpoint = a.checkpoint();

    let reps = if (op.flags & LIMIT_REPS) == 0 { 3 } else { 1 };
    let mut avg_factor = Average::new();
    for _k in 0..reps {
        for i in 1..100 {
            a.restore_checkpoint(&checkpoint);
            let call = build_nested_call(a, op.opcode, arg, i, op.extra);
            let (time, raw_time, cost) = time_invocation(a, call, op);
            avg_factor.add(cost as f64 / raw_time);
            let duration = time - (per_arg_time * i as f64);
            let sample = (i as f64, duration);
            writeln!(output, "{i}\t{duration}\t{raw_time}\t{cost}").expect("failed to write");
            samples.push(sample);
        }
    }

    if (op.flags & PLOT_COST) != 0 {
        println!("   (base-call) cost/ns: {}", avg_factor.compute());
    }

    let (slope, _): (f64, f64) = linear_regression_of(&samples).expect("linreg failed");
    slope
}

fn base_call_time_no_nest(a: &mut Allocator, op: &Operator, per_arg_time: f64) -> f64 {
    let subst = a
        .new_atom(
            &hex::decode("123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0")
                .unwrap(),
        )
        .unwrap();
    let arg = substitute(op.arg, &mut || quote(a, subst));

    let checkpoint = a.checkpoint();

    let mut average_time = Average::new();
    let mut avg_factor = Average::new();
    for _i in 0..300 {
        a.restore_checkpoint(&checkpoint);
        let call = build_call(a, op.opcode, arg, 1, None);
        let (_time, raw_time, cost) = time_invocation(a, call, op);
        avg_factor.add(cost as f64 / raw_time);
        average_time.add(raw_time - per_arg_time);
    }

    if (op.flags & PLOT_COST) != 0 {
        println!("   (base-call) cost/ns: {}", avg_factor.compute());
    }

    average_time.compute()
}

// measures run-time of many calls with the variable argument substituted for
// buffers of variying sizes
const PER_BYTE_COST: u32 = 1;

// measures the run-time of many calls with varying number of arguments, to
// establish how much time each additional argument contributes
const PER_ARG_COST: u32 = 2;

// measure run-time of many *nested* calls, to establish how much longer it
// takes for each additional nested call. The per_arg_time is subtracted to get
// the base cost. This only works for operators that can take their return
// value as an argument
const NESTING_BASE_COST: u32 = 4;

// By default, arguments passed to per-byte timing contain random values. If one
// of these flags are set, we instead use values with many zero bits or many 1
// bits.
const MANY_ONES_ARG: u32 = 8;
const MANY_ZERO_ARG: u32 = 16;

// Allow the operator to fail. This is useful for signature validation
// functions. They must take just as long to execute as a successful run for this
// to work as expected.
const ALLOW_FAILURE: u32 = 32;

// For expensive operations, we can limit the number of measurements with this
// flag
const LIMIT_REPS: u32 = 64;

// In addition to plotting the time measurements of operators, also plot the
// cost, as reported by CLVM. This makes sense for operators that have already
// been deployed (or to validate that a model seems to fit the measurements)
const PLOT_COST: u32 = 128;

// make sure arguments are positive
const POSITIVE_ARGS: u32 = 256;

struct Operator {
    opcode: u32,
    name: &'static str,
    arg: Placeholder,
    extra: Option<NodePtr>,
    // scale up the argument atom sizes by this factor
    arg_scale: usize,
    // for non-linear cost models, modify each time sample by finding the n:th
    // root, where this field specify n. 1 means unchanged, 2 means square root
    root: u32,
    flags: u32,
}

/// Measure CPU cost of CLVM operators to aid in determining their cost
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Only benchmark a single operator, by specifying its name
    #[arg(long)]
    only_operator: Option<String>,

    /// Multiply timings (in nanoseconds) by this factor to get cost
    #[arg(long)]
    cost_factor: Option<f64>,

    /// List available operators, by name, and exit
    #[arg(long)]
    list_operators: bool,

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

fn write_gnuplot_header(
    gnuplot: &mut dyn Write,
    op: &Operator,
    out: &str,
    xlabel: &str,
    title: &str,
    y2: bool,
) {
    writeln!(
        gnuplot,
        "set output \"{}-{out}.svg\"
set title \"{} {title}\"
set xlabel \"{xlabel}\"
set ylabel \"nanoseconds{}\"
set ytics nomirror",
        op.name,
        op.name,
        if op.root != 1 && !y2 {
            format!(" log{}", op.root)
        } else {
            String::new()
        }
    )
    .expect("failed to write");

    if y2 {
        writeln!(
            gnuplot,
            "set y2label \"cost\"
set y2tics"
        )
        .expect("failed to write");
    } else {
        writeln!(
            gnuplot,
            "unset y2label
unset y2tics"
        )
        .expect("failed to write");
    }
}

fn print_plot(gnuplot: &mut dyn Write, a: &f64, b: &f64, op: &str, name: &str) {
    writeln!(gnuplot, "f(x) = {a}*x+{b}").expect("failed to write");
    writeln!(
        gnuplot,
        "plot \"{op}-{name}.log\" using 1:2 with dots title \"{name} (measured)\", f(x) title \"{name} (fitted)\""
    )
    .expect("failed to write");
}

fn print_plot2(gnuplot: &mut dyn Write, op: &str, name: &str, cost_factor: Option<f64>) {
    write!(
        gnuplot,
        "plot \"{op}-{name}.log\" using 1:3 with dots title \"{name} (measured)\","
    )
    .expect("failed to write");
    write!(
        gnuplot,
        "\"{op}-{name}.log\" using 1:4 with dots axis x1y2 title \"{name} (CLVM cost)\""
    )
    .expect("failed to write");
    if let Some(cost_scale) = cost_factor {
        write!(gnuplot, ", \"{op}-{name}.log\" using 1:($4/{cost_scale}) with dots axis x1y2 title \"{name} timing inferred by cost-factor\"")
        .expect("failed to write");
    }
    writeln!(gnuplot).expect("failed to write");
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
            opcode: 18,
            name: "mul",
            arg: Placeholder::TwoArgs(None, None),
            arg_scale: 5,
            root: 2,
            extra: None,
            flags: PER_BYTE_COST | PLOT_COST,
        },
        Operator {
            opcode: 60,
            name: "modpow (modulus cost)",
            arg: Placeholder::ThreeArgs(Some(number), Some(number), None),
            arg_scale: 2,
            root: 3,
            extra: None,
            flags: PER_BYTE_COST | PLOT_COST | POSITIVE_ARGS | LIMIT_REPS | MANY_ZERO_ARG,
        },
        Operator {
            opcode: 60,
            name: "modpow (exponent cost)",
            arg: Placeholder::ThreeArgs(Some(number), None, Some(number)),
            arg_scale: 2,
            root: 2,
            extra: None,
            flags: PER_BYTE_COST | PLOT_COST | POSITIVE_ARGS | LIMIT_REPS | MANY_ZERO_ARG,
        },
        Operator {
            opcode: 60,
            name: "modpow (value cost)",
            arg: Placeholder::ThreeArgs(None, Some(number), Some(number)),
            arg_scale: 2,
            root: 2,
            extra: None,
            flags: PER_BYTE_COST | PLOT_COST | POSITIVE_ARGS | LIMIT_REPS | MANY_ZERO_ARG,
        },
        Operator {
            opcode: 29,
            name: "point_add",
            arg: Placeholder::SingleArg(Some(g1)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST | LIMIT_REPS | PLOT_COST,
        },
        Operator {
            opcode: 49,
            name: "g1_subtract",
            arg: Placeholder::SingleArg(Some(g1)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST | LIMIT_REPS | PLOT_COST,
        },
        Operator {
            opcode: 50,
            name: "g1_multiply",
            arg: Placeholder::TwoArgs(Some(g1), None),
            arg_scale: 1,
            root: 1,
            extra: Some(g1),
            flags: PER_BYTE_COST | PLOT_COST,
        },
        Operator {
            opcode: 51,
            name: "g1_negate",
            arg: Placeholder::SingleArg(Some(g1)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PLOT_COST,
        },
        Operator {
            opcode: 52,
            name: "g2_add",
            arg: Placeholder::SingleArg(Some(g2)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST | LIMIT_REPS | PLOT_COST,
        },
        Operator {
            opcode: 53,
            name: "g2_subtract",
            arg: Placeholder::SingleArg(Some(g2)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PER_ARG_COST | NESTING_BASE_COST | LIMIT_REPS | PLOT_COST,
        },
        Operator {
            opcode: 54,
            name: "g2_multiply",
            arg: Placeholder::TwoArgs(Some(g2), None),
            arg_scale: 1,
            root: 1,
            extra: Some(g2),
            flags: PER_BYTE_COST | PLOT_COST,
        },
        Operator {
            opcode: 55,
            name: "g2_negate",
            arg: Placeholder::SingleArg(Some(g2)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PLOT_COST,
        },
        Operator {
            opcode: 56,
            name: "g1_map",
            arg: Placeholder::SingleArg(None),
            arg_scale: 1000,
            root: 1,
            extra: None,
            flags: PER_BYTE_COST | PLOT_COST,
        },
        Operator {
            opcode: 57,
            name: "g2_map",
            arg: Placeholder::SingleArg(None),
            arg_scale: 1000,
            root: 1,
            extra: None,
            flags: PER_BYTE_COST | PLOT_COST,
        },
        Operator {
            opcode: 58,
            name: "bls_pairing_identity",
            arg: Placeholder::TwoArgs(Some(g1), Some(g2)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: PER_ARG_COST | ALLOW_FAILURE | LIMIT_REPS,
        },
        Operator {
            opcode: 59,
            name: "bls_verify",
            arg: Placeholder::TwoArgs(Some(g1), Some(g2)),
            arg_scale: 1,
            root: 1,
            extra: Some(g2),
            flags: PER_ARG_COST | ALLOW_FAILURE | LIMIT_REPS,
        },
        Operator {
            opcode: 0x13d61f00,
            name: "secp256k1_verify",
            arg: Placeholder::ThreeArgs(Some(k1_pk), Some(k1_msg), Some(k1_sig)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: ALLOW_FAILURE,
        },
        Operator {
            opcode: 0x1c3a8f00,
            name: "secp256r1_verify",
            arg: Placeholder::ThreeArgs(Some(r1_pk), Some(r1_msg), Some(r1_sig)),
            arg_scale: 1,
            root: 1,
            extra: None,
            flags: ALLOW_FAILURE,
        },
        Operator {
            opcode: 11,
            name: "sha256",
            arg: Placeholder::SingleArg(None),
            arg_scale: 1000,
            root: 1,
            extra: None,
            flags: NESTING_BASE_COST | PER_ARG_COST | PER_BYTE_COST | PLOT_COST,
        },
        Operator {
            opcode: 62,
            name: "keccak256",
            arg: Placeholder::SingleArg(Some(g1)),
            arg_scale: 1000,
            root: 1,
            extra: None,
            flags: NESTING_BASE_COST | PER_ARG_COST | PER_BYTE_COST | PLOT_COST,
        },
    ];

    if options.list_operators {
        for op in &ops {
            println!("{}", op.name);
        }
        return;
    }

    if let Some(cost_scale) = options.cost_factor {
        println!("cost scale: {cost_scale}");
    }

    let mut gnuplot = maybe_open(options.plot, "gen", "graphs.gnuplot");
    writeln!(gnuplot, "set term svg").expect("failed to write");
    writeln!(gnuplot, "set key top left").expect("failed to write");

    for op in &ops {
        // If an operator name was specified, skip all other operators
        if let Some(ref name) = options.only_operator
            && op.name != name
        {
            continue;
        }

        println!("opcode: {} ({})", op.name, op.opcode);
        let time_per_byte = if (op.flags & PER_BYTE_COST) != 0 {
            let mut output = maybe_open(options.plot, op.name, "per-byte.log");
            write_gnuplot_header(
                &mut *gnuplot,
                op,
                "per-byte",
                "num bytes",
                "timing per-byte, argument",
                false,
            );
            let time_per_byte = time_per_byte(&mut a, op, &mut *output);
            println!("   time: per-byte: {time_per_byte:.2}ns");
            if let Some(cost_scale) = options.cost_factor {
                println!(
                    "   estimated-cost: per-byte: {:.2}",
                    time_per_byte * cost_scale
                );
            }
            print_plot(&mut *gnuplot, &time_per_byte, &0.0, op.name, "per-byte");
            time_per_byte
        } else {
            0.0
        };
        let time_per_arg = if (op.flags & PER_ARG_COST) != 0 {
            let mut output = maybe_open(options.plot, op.name, "per-arg.log");
            let time_per_arg = time_per_arg(&mut a, op, &mut *output);
            println!("   time: per-arg: {time_per_arg:.2}ns");
            if let Some(cost_scale) = options.cost_factor {
                println!(
                    "   estimated-cost: per-arg: {:.2}",
                    time_per_arg * cost_scale
                );
            }
            time_per_arg
        } else {
            0.0
        };
        let base_call_time = if (op.flags & NESTING_BASE_COST) != 0 {
            let mut output = maybe_open(options.plot, op.name, "base.log");
            write_gnuplot_header(
                &mut *gnuplot,
                op,
                "base",
                "num nested calls",
                "base cost, nested calls",
                false,
            );
            let base_call_time = base_call_time(&mut a, op, time_per_arg, &mut *output);
            println!("   time: base: {base_call_time:.2}ns");
            if let Some(cost_scale) = options.cost_factor {
                println!(
                    "   estimated-cost: base: {:.2}",
                    base_call_time * cost_scale
                );
            }

            print_plot(&mut *gnuplot, &base_call_time, &0.0, op.name, "base");
            base_call_time
        } else {
            let base_call_time = base_call_time_no_nest(&mut a, op, time_per_arg);
            println!("   time: base: {base_call_time:.2}ns");
            if let Some(cost_scale) = options.cost_factor {
                println!(
                    "   estimated-cost: base: {:.2}",
                    base_call_time * cost_scale
                );
            }
            base_call_time
        };

        // we adjust the base_call_time here to make the curve fitting match
        let base_call_time = if op.root != 1 {
            base_call_time.powf(1.0 / (op.root as f64))
        } else {
            base_call_time
        };
        if (op.flags & PER_ARG_COST) != 0 {
            write_gnuplot_header(
                &mut *gnuplot,
                op,
                "per-arg",
                "num arguments",
                "timing per argument",
                false,
            );
            print_plot(
                &mut *gnuplot,
                &time_per_arg,
                &base_call_time,
                op.name,
                "per-arg",
            );
        }
        if (op.flags & PER_BYTE_COST) != 0 {
            write_gnuplot_header(
                &mut *gnuplot,
                op,
                "per-byte",
                "num bytes",
                "timing per byte",
                false,
            );
            print_plot(
                &mut *gnuplot,
                &time_per_byte,
                &base_call_time,
                op.name,
                "per-byte",
            );
        }

        // This is for plotting similar graphs, along with the actual cost
        // reported by CLVM
        if (op.flags & PLOT_COST) != 0 {
            if (op.flags & PER_ARG_COST) != 0 {
                write_gnuplot_header(
                    &mut *gnuplot,
                    op,
                    "per-arg-cost",
                    "num arguments",
                    "per argument",
                    true,
                );
                print_plot2(&mut gnuplot, op.name, "per-arg", options.cost_factor);
            }
            if (op.flags & NESTING_BASE_COST) != 0 {
                write_gnuplot_header(
                    &mut *gnuplot,
                    op,
                    "base-cost",
                    "num nested calls",
                    "base cost, nested calls",
                    true,
                );
                print_plot2(&mut gnuplot, op.name, "base", options.cost_factor);
            }
            if (op.flags & PER_BYTE_COST) != 0 {
                write_gnuplot_header(
                    &mut *gnuplot,
                    op,
                    "per-byte-cost",
                    "num bytes",
                    "per byte",
                    true,
                );
                print_plot2(&mut gnuplot, op.name, "per-byte", options.cost_factor);
            }
        }
    }
    if options.plot {
        println!("To generate plots, run:\n   (cd measurements; gnuplot gen-graphs.gnuplot)");
    }
}
