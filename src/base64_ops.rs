use crate::allocator::{Allocator, Atom, NodePtr};
use crate::cost::check_cost;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::op_utils::{atom_len, get_args, new_atom_and_cost, MALLOC_COST_PER_BYTE};
use crate::reduction::{EvalErr, Reduction, Response};
use crate::SExp;
use base64ct::{Base64UrlUnpadded, Encoder, Encoding};

const BASE64_ENCODE_BASE_COST: Cost = 40;
const BASE64_COST_PER_ARG: Cost = 130;
const BASE64_DECODE_BASE_COST: Cost = 400;
const BASE64_COST_PER_BYTE: Cost = 3;

// this computes the size of the resulting base64 encoded string, given a number
// of input bytes. We pre-allocate the string.
const fn encoded_len(n: usize) -> Option<usize> {
    let Some(q) = n.checked_mul(4) else {
        return None;
    };
    Some((q / 3) + (q % 3 != 0) as usize)
}

pub fn op_base64url_encode(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BASE64_ENCODE_BASE_COST;
    let mut input_size: usize = 0;

    let mut arg_list = input;
    while let Some((arg, rest)) = a.next(arg_list) {
        arg_list = rest;
        cost += BASE64_COST_PER_ARG;
        let len = match a.sexp(arg) {
            SExp::Pair(_, _) => return err(arg, "base64url on list"),
            SExp::Atom => a.atom_len(arg),
        };

        input_size += len;
        cost += len as Cost * BASE64_COST_PER_BYTE;
        check_cost(a, cost, max_cost)?;
    }

    if input_size == 0 {
        return Ok(Reduction(cost, a.nil()));
    }

    let output_size =
        encoded_len(input_size).ok_or(EvalErr(input, "base64 invalid input length".to_string()))?;

    cost += output_size as Cost * MALLOC_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;

    let mut output: Vec<u8> = vec![0; output_size];
    let mut enc = Encoder::<Base64UrlUnpadded>::new(&mut output[..])
        .map_err(|_| EvalErr(input, "base64 (internal error)".to_string()))?;

    let mut arg_list = input;
    while let Some((arg, rest)) = a.next(arg_list) {
        arg_list = rest;
        match a.atom(arg) {
            Atom::Borrowed(b) => {
                enc.encode(b)
                    .map_err(|_| EvalErr(arg, "base64 (internal error)".to_string()))?;
            }
            Atom::U32(b, len) => {
                enc.encode(&b[4 - len..])
                    .map_err(|_| EvalErr(arg, "base64 (internal error)".to_string()))?;
            }
        };
    }

    enc.finish()
        .map_err(|_| EvalErr(input, "base64 (internal error)".to_string()))?;
    let new_atom = a.new_atom(&output)?;
    Ok(Reduction(cost, new_atom))
}

pub fn op_base64url_decode(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [input] = get_args::<1>(a, input, "base64url_decode")?;
    let input_size = atom_len(a, input, "base64url_decode")?;
    let cost = BASE64_DECODE_BASE_COST + input_size as Cost * BASE64_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;

    if input_size == 0 {
        return Ok(Reduction(cost, a.nil()));
    }

    let encoded = a.atom(input);
    let encoded = std::str::from_utf8(encoded.as_ref())
        .map_err(|_| EvalErr(input, "base64url_decode (invalid input)".to_string()))?;
    let output = Base64UrlUnpadded::decode_vec(encoded)
        .map_err(|_| EvalErr(input, "base64url_decode (invalid input)".to_string()))?;

    new_atom_and_cost(a, cost, &output)
}
