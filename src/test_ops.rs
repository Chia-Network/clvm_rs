use crate::allocator::{Allocator, NodePtr, SExp};
use crate::bls_ops::{
    op_bls_g1_multiply, op_bls_g1_negate, op_bls_g1_subtract, op_bls_g2_add, op_bls_g2_multiply,
    op_bls_g2_negate, op_bls_g2_subtract, op_bls_map_to_g1, op_bls_map_to_g2,
    op_bls_pairing_identity, op_bls_verify,
};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_coinid, op_concat, op_div, op_divmod, op_gr, op_gr_bytes,
    op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_mod, op_modpow, op_multiply, op_not,
    op_point_add, op_pubkey_for_exp, op_sha256, op_strlen, op_substr, op_subtract,
};
use crate::number::Number;
use crate::reduction::{EvalErr, Reduction, Response};
use crate::secp_ops::{op_secp256k1_verify, op_secp256r1_verify};

use hex::FromHex;
use num_traits::Num;
use std::cmp::min;
use std::collections::HashMap;

fn parse_atom(a: &mut Allocator, v: &str) -> NodePtr {
    if v == "0" {
        return a.null();
    }

    assert!(!v.is_empty());

    if v.starts_with("0x") {
        let buf = Vec::from_hex(v.strip_prefix("0x").unwrap()).unwrap();
        return a.new_atom(&buf).unwrap();
    }

    if v.starts_with('\"') {
        assert!(v.ends_with('\"'));
        let buf = v
            .strip_prefix('\"')
            .unwrap()
            .strip_suffix('\"')
            .unwrap()
            .as_bytes();
        return a.new_atom(buf).unwrap();
    }

    if let Ok(num) = Number::from_str_radix(v, 10) {
        a.new_number(num).unwrap()
    } else {
        let v = v.strip_prefix('#').unwrap_or(v);
        match v {
            "q" => a.new_atom(&[1]).unwrap(),
            "a" => a.new_atom(&[2]).unwrap(),
            "i" => a.new_atom(&[3]).unwrap(),
            "c" => a.new_atom(&[4]).unwrap(),
            "f" => a.new_atom(&[5]).unwrap(),
            "r" => a.new_atom(&[6]).unwrap(),
            "l" => a.new_atom(&[7]).unwrap(),
            "x" => a.new_atom(&[8]).unwrap(),
            "=" => a.new_atom(&[9]).unwrap(),
            ">s" => a.new_atom(&[10]).unwrap(),
            "sha256" => a.new_atom(&[11]).unwrap(),
            "substr" => a.new_atom(&[12]).unwrap(),
            "strlen" => a.new_atom(&[13]).unwrap(),
            "concat" => a.new_atom(&[14]).unwrap(),

            "+" => a.new_atom(&[16]).unwrap(),
            "-" => a.new_atom(&[17]).unwrap(),
            "*" => a.new_atom(&[18]).unwrap(),
            "/" => a.new_atom(&[19]).unwrap(),
            "divmod" => a.new_atom(&[20]).unwrap(),
            ">" => a.new_atom(&[21]).unwrap(),
            "ash" => a.new_atom(&[22]).unwrap(),
            "lsh" => a.new_atom(&[23]).unwrap(),
            "logand" => a.new_atom(&[24]).unwrap(),
            "logior" => a.new_atom(&[25]).unwrap(),
            "logxor" => a.new_atom(&[26]).unwrap(),
            "lognot" => a.new_atom(&[27]).unwrap(),

            "point_add" => a.new_atom(&[29]).unwrap(),
            "pubkey_for_exp" => a.new_atom(&[30]).unwrap(),

            "not" => a.new_atom(&[32]).unwrap(),
            "any" => a.new_atom(&[33]).unwrap(),
            "all" => a.new_atom(&[34]).unwrap(),

            "softfork" => a.new_atom(&[36]).unwrap(),

            "coinid" => a.new_atom(&[48]).unwrap(),

            "g1_add" => a.new_atom(&[29]).unwrap(),
            "g1_subtract" => a.new_atom(&[49]).unwrap(),
            "g1_multiply" => a.new_atom(&[50]).unwrap(),
            "g1_negate" => a.new_atom(&[51]).unwrap(),
            "g2_add" => a.new_atom(&[52]).unwrap(),
            "g2_subtract" => a.new_atom(&[53]).unwrap(),
            "g2_multiply" => a.new_atom(&[54]).unwrap(),
            "g2_negate" => a.new_atom(&[55]).unwrap(),
            "g1_map" => a.new_atom(&[56]).unwrap(),
            "g2_map" => a.new_atom(&[57]).unwrap(),
            "bls_pairing_identity" => a.new_atom(&[58]).unwrap(),
            "bls_verify" => a.new_atom(&[59]).unwrap(),
            "secp256k1_verify" => a.new_atom(&[0x13, 0xd6, 0x1f, 0x00]).unwrap(),
            "secp256r1_verify" => a.new_atom(&[0x1c, 0x3a, 0x8f, 0x00]).unwrap(),
            _ => {
                panic!("atom not supported \"{}\"", v);
            }
        }
    }
}

fn pop_token(s: &str) -> (&str, &str) {
    let s = s.trim();
    if let Some(stripped) = s.strip_prefix('\"') {
        if let Some(second_quote) = stripped.find('\"') {
            let (first, rest) = s.split_at(second_quote + 2);
            (first.trim(), rest.trim())
        } else {
            panic!("mismatching quote")
        }
    } else if s.starts_with('(') || s.starts_with(')') {
        let (first, rest) = s.split_at(1);
        (first, rest.trim())
    } else {
        let space = s.find(' ');
        let close = s.find(')');

        let split_pos = if let (Some(space_pos), Some(close_pos)) = (space, close) {
            min(space_pos, close_pos)
        } else if let Some(pos) = space {
            pos
        } else if let Some(pos) = close {
            pos
        } else {
            s.len()
        };

        let (first, rest) = s.split_at(split_pos);
        (first.trim(), rest.trim())
    }
}

pub fn parse_list<'a>(a: &mut Allocator, v: &'a str) -> (NodePtr, &'a str) {
    let v = v.trim();
    let (first, rest) = pop_token(v);
    if first.is_empty() {
        return (a.null(), rest);
    }
    if first == ")" {
        return (a.null(), rest);
    }
    if first == "(" {
        let (head, new_rest) = parse_list(a, rest);
        let (tail, new_rest) = parse_list(a, new_rest);
        (a.new_pair(head, tail).unwrap(), new_rest)
    } else if first == "." {
        let (node, new_rest) = parse_exp(a, rest);
        let (end_list, new_rest) = pop_token(new_rest);
        assert_eq!(end_list, ")");
        (node, new_rest)
    } else {
        let head = parse_atom(a, first);
        let (tail, new_rest) = parse_list(a, rest);
        (a.new_pair(head, tail).unwrap(), new_rest)
    }
}

pub fn parse_exp<'a>(a: &mut Allocator, v: &'a str) -> (NodePtr, &'a str) {
    let (first, rest) = pop_token(v);
    if first == "(" {
        parse_list(a, rest)
    } else {
        (parse_atom(a, first), rest)
    }
}

pub fn node_eq(allocator: &Allocator, s1: NodePtr, s2: NodePtr) -> bool {
    match (allocator.sexp(s1), allocator.sexp(s2)) {
        (SExp::Pair(s1a, s1b), SExp::Pair(s2a, s2b)) => {
            node_eq(allocator, s1a, s2a) && node_eq(allocator, s1b, s2b)
        }
        (SExp::Atom, SExp::Atom) => allocator.atom_eq(s1, s2),
        _ => false,
    }
}

type Opf = fn(&mut Allocator, NodePtr, Cost) -> Response;

// the input is a list of test cases, each item is a tuple of:
// (function pointer to test, list of arguments, optional result)
// if the result is None, the call is expected to fail
fn run_op_test(op: &Opf, args_str: &str, expected: &str, expected_cost: u64) {
    let mut a = Allocator::new();

    let (args, rest) = parse_list(&mut a, args_str);
    assert_eq!(rest, "");
    let result = op(&mut a, args, 10000000000 as Cost);
    match result {
        Err(e) => {
            println!("Error: {}", e.1);
            assert_eq!(expected, "FAIL");
        }
        Ok(Reduction(cost, ret_value)) => {
            assert_eq!(cost, expected_cost);
            let (expected, rest) = parse_exp(&mut a, expected);
            assert_eq!(rest, "");
            assert!(node_eq(&a, ret_value, expected));
        }
    }
}

#[cfg(test)]
use rstest::rstest;

#[cfg(test)]
#[rstest]
#[case("test-core-ops")]
#[case("test-more-ops")]
#[case("test-bls-ops")]
#[case("test-blspy-g1")]
#[case("test-blspy-g2")]
#[case("test-blspy-hash")]
#[case("test-blspy-pairing")]
#[case("test-blspy-verify")]
#[case("test-bls-zk")]
#[case("test-secp-verify")]
#[case("test-secp256k1")]
#[case("test-secp256r1")]
#[case("test-modpow")]
fn test_ops(#[case] filename: &str) {
    use std::fs::read_to_string;

    let filename = format!("op-tests/{filename}.txt");

    let funs = HashMap::from([
        ("i", op_if as Opf),
        ("c", op_cons as Opf),
        ("f", op_first as Opf),
        ("r", op_rest as Opf),
        ("l", op_listp as Opf),
        ("x", op_raise as Opf),
        ("=", op_eq as Opf),
        ("sha256", op_sha256 as Opf),
        ("+", op_add as Opf),
        ("-", op_subtract as Opf),
        ("*", op_multiply as Opf),
        ("/", op_div as Opf),
        ("divmod", op_divmod as Opf),
        ("%", op_mod as Opf),
        ("substr", op_substr as Opf),
        ("strlen", op_strlen as Opf),
        ("point_add", op_point_add as Opf),
        ("pubkey_for_exp", op_pubkey_for_exp as Opf),
        ("concat", op_concat as Opf),
        (">", op_gr as Opf),
        (">s", op_gr_bytes as Opf),
        ("logand", op_logand as Opf),
        ("logior", op_logior as Opf),
        ("logxor", op_logxor as Opf),
        ("lognot", op_lognot as Opf),
        ("ash", op_ash as Opf),
        ("lsh", op_lsh as Opf),
        ("not", op_not as Opf),
        ("any", op_any as Opf),
        ("all", op_all as Opf),
        //the BLS extension
        ("coinid", op_coinid as Opf),
        ("g1_add", op_point_add as Opf),
        ("g1_subtract", op_bls_g1_subtract as Opf),
        ("g1_multiply", op_bls_g1_multiply as Opf),
        ("g1_negate", op_bls_g1_negate as Opf),
        ("g2_add", op_bls_g2_add as Opf),
        ("g2_subtract", op_bls_g2_subtract as Opf),
        ("g2_multiply", op_bls_g2_multiply as Opf),
        ("g2_negate", op_bls_g2_negate as Opf),
        ("g1_map", op_bls_map_to_g1 as Opf),
        ("g2_map", op_bls_map_to_g2 as Opf),
        ("bls_pairing_identity", op_bls_pairing_identity as Opf),
        ("bls_verify", op_bls_verify as Opf),
        ("secp256k1_verify", op_secp256k1_verify as Opf),
        ("secp256r1_verify", op_secp256r1_verify as Opf),
        ("modpow", op_modpow as Opf),
    ]);

    println!("Test cases from: {filename}");
    let test_cases = read_to_string(filename).expect("test file not found");
    for t in test_cases.split('\n') {
        let t = t.trim();
        if t.is_empty() {
            continue;
        }
        // ignore comments
        if t.starts_with(';') {
            continue;
        }
        let (op_name, t) = t.split_once(' ').unwrap();
        let op = funs
            .get(op_name)
            .unwrap_or_else(|| panic!("couldn't find operator \"{op_name}\""));
        let (args, out) = t.split_once("=>").unwrap();
        let (expected, expected_cost) = if out.contains('|') {
            out.split_once('|').unwrap()
        } else {
            (out, "0")
        };

        println!("({} {}) => {}", op_name, args.trim(), expected.trim());
        run_op_test(
            op,
            args.trim(),
            expected.trim(),
            expected_cost.trim().parse().unwrap(),
        );
    }
}

#[test]
fn test_single_argument_raise_atom() {
    let mut allocator = Allocator::new();
    let a1 = allocator.new_atom(&[65]).unwrap();
    let args = allocator.new_pair(a1, allocator.null()).unwrap();
    let result = op_raise(&mut allocator, args, 100000);
    assert_eq!(result, Err(EvalErr(a1, "clvm raise".to_string())));
}

#[test]
fn test_single_argument_raise_pair() {
    let mut allocator = Allocator::new();
    let a1 = allocator.new_atom(&[65]).unwrap();
    let a2 = allocator.new_atom(&[66]).unwrap();
    // (a2)
    let mut args = allocator.new_pair(a2, allocator.null()).unwrap();
    // (a1 a2)
    args = allocator.new_pair(a1, args).unwrap();
    // ((a1 a2))
    args = allocator.new_pair(args, allocator.null()).unwrap();
    let result = op_raise(&mut allocator, args, 100000);
    assert_eq!(result, Err(EvalErr(args, "clvm raise".to_string())));
}

#[test]
fn test_multi_argument_raise() {
    let mut allocator = Allocator::new();
    let a1 = allocator.new_atom(&[65]).unwrap();
    let a2 = allocator.new_atom(&[66]).unwrap();
    // (a1)
    let mut args = allocator.new_pair(a2, allocator.null()).unwrap();
    // (a1 a2)
    args = allocator.new_pair(a1, args).unwrap();
    let result = op_raise(&mut allocator, args, 100000);
    assert_eq!(result, Err(EvalErr(args, "clvm raise".to_string())));
}

#[cfg(feature = "pre-eval")]
const COST_LIMIT: u64 = 1000000000;

#[cfg(feature = "pre-eval")]
struct EvalFTracker {
    pub prog: NodePtr,
    pub args: NodePtr,
    pub outcome: Option<NodePtr>,
}

#[cfg(feature = "pre-eval")]
use crate::chia_dialect::{ChiaDialect, NO_UNKNOWN_OPS};
#[cfg(feature = "pre-eval")]
use crate::run_program::run_program_with_pre_eval;
#[cfg(feature = "pre-eval")]
use std::cell::RefCell;
#[cfg(feature = "pre-eval")]
use std::collections::HashSet;

// Allows move closures to tear off a reference and move it. // Allows interior
// mutability inside Fn traits.
#[cfg(feature = "pre-eval")]
use std::rc::Rc;

// Ensure pre_eval_f and post_eval_f are working as expected.
#[cfg(feature = "pre-eval")]
#[test]
fn test_pre_eval_and_post_eval() {
    let mut allocator = Allocator::new();

    let a1 = allocator.new_atom(&[1]).unwrap();
    let a2 = allocator.new_atom(&[2]).unwrap();
    let a4 = allocator.new_atom(&[4]).unwrap();
    let a5 = allocator.new_atom(&[5]).unwrap();

    let a99 = allocator.new_atom(&[99]).unwrap();
    let a101 = allocator.new_atom(&[101]).unwrap();

    // (a (q . (f (c 2 5))) (q 99 101))
    let arg_tail = allocator.new_pair(a101, allocator.null()).unwrap();
    let arg_mid = allocator.new_pair(a99, arg_tail).unwrap();
    let args = allocator.new_pair(a1, arg_mid).unwrap();

    let cons_tail = allocator.new_pair(a5, allocator.null()).unwrap();
    let cons_args = allocator.new_pair(a2, cons_tail).unwrap();
    let cons_expr = allocator.new_pair(a4, cons_args).unwrap();

    let f_tail = allocator.new_pair(cons_expr, allocator.null()).unwrap();
    let f_expr = allocator.new_pair(a5, f_tail).unwrap();
    let f_quoted = allocator.new_pair(a1, f_expr).unwrap();

    let a_tail = allocator.new_pair(args, allocator.null()).unwrap();
    let a_args = allocator.new_pair(f_quoted, a_tail).unwrap();
    let program = allocator.new_pair(a2, a_args).unwrap();

    let tracking = Rc::new(RefCell::new(HashMap::new()));
    let pre_eval_tracking = tracking.clone();
    let pre_eval_f: Box<
        dyn Fn(
            &mut Allocator,
            NodePtr,
            NodePtr,
        ) -> Result<Option<Box<(dyn Fn(Option<NodePtr>))>>, EvalErr>,
    > = Box::new(move |_allocator, prog, args| {
        let tracking_key = pre_eval_tracking.borrow().len();
        // Ensure lifetime of mutable borrow is contained.
        // It must end before the lifetime of the following closure.
        {
            let mut tracking_mutable = pre_eval_tracking.borrow_mut();
            tracking_mutable.insert(
                tracking_key,
                EvalFTracker {
                    prog,
                    args,
                    outcome: None,
                },
            );
        }
        let post_eval_tracking = pre_eval_tracking.clone();
        let post_eval_f: Box<dyn Fn(Option<NodePtr>)> = Box::new(move |outcome| {
            let mut tracking_mutable = post_eval_tracking.borrow_mut();
            tracking_mutable.insert(
                tracking_key,
                EvalFTracker {
                    prog,
                    args,
                    outcome,
                },
            );
        });
        Ok(Some(post_eval_f))
    });

    let allocator_null = allocator.null();
    let result = run_program_with_pre_eval(
        &mut allocator,
        &ChiaDialect::new(NO_UNKNOWN_OPS),
        program,
        allocator_null,
        COST_LIMIT,
        Some(pre_eval_f),
    )
    .unwrap();

    assert!(node_eq(&allocator, result.1, a99));

    // Should produce these:
    // (q 99 101) => (99 101)
    // (q . (f (c 2 5))) => (f (c 2 5))
    // 2 (99 101) => 99
    // 5 (99 101) => 101
    // (c 2 5) (99 101) => (99 . 101)
    // (f (c 2 5)) (99 101) => 99
    // (a (q 5 (c 2 5))) () => 99

    // args consed
    let args_consed = allocator.new_pair(a99, a101).unwrap();

    let mut desired_outcomes = Vec::new(); // Not in order.
    desired_outcomes.push((args, allocator_null, arg_mid));
    desired_outcomes.push((f_quoted, allocator_null, f_expr));
    desired_outcomes.push((a2, arg_mid, a99));
    desired_outcomes.push((a5, arg_mid, a101));
    desired_outcomes.push((cons_expr, arg_mid, args_consed));
    desired_outcomes.push((f_expr, arg_mid, a99));
    desired_outcomes.push((program, allocator_null, a99));

    let mut found_outcomes = HashSet::new();
    let tracking_examine = tracking.borrow();
    for (_, v) in tracking_examine.iter() {
        let found = desired_outcomes.iter().position(|(p, a, o)| {
            node_eq(&allocator, *p, v.prog)
                && node_eq(&allocator, *a, v.args)
                && node_eq(&allocator, v.outcome.unwrap(), *o)
        });
        found_outcomes.insert(found);
        assert!(found.is_some());
    }

    assert_eq!(tracking_examine.len(), desired_outcomes.len());
    assert_eq!(tracking_examine.len(), found_outcomes.len());
}
