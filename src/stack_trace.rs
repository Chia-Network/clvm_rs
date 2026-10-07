//! A read-only, post-failure decoder for CLVM evaluation stack traces.
//!
//! At the moment `run_program`'s eval loop returns `Err`, the evaluator's own
//! `op_stack` / `val_stack` / `env_stack` still encode the full in-flight
//! call structure (the loop propagates errors via `?` and never drains the
//! stacks). This module reconstructs a failure trace from that residue
//! *after the fact*, with no runtime bookkeeping at all: no new `Operation`
//! payload, no per-eval cost, nothing evicted.
//!
//! See `run_program::run_program_with_diagnostics` for the public entry point
//! that produces the [`StackSnapshot`] this module consumes. In tests built
//! with the `pre-eval` feature, this decoder is cross-validated against a
//! capture-based oracle built on top of `run_program_with_pre_eval` (see
//! `stack_trace_tests`).
//!
//! ## The stack discipline this decoder relies on
//!
//! For one in-flight, non-leaf evaluation frame (an operator call with
//! operator `op_node` and `N` operands), reading `op_stack` from the bottom
//! (oldest) up to the top (newest) within that frame's own region:
//!
//! ```text
//! [Apply] [SwapEval(op_0)] [SwapEval(op_1)] ... [SwapEval(op_{N-1})]
//! ```
//!
//! Operands are evaluated in *reverse* source order (last operand first:
//! `swap_eval_op` pops the most-recently-pushed pending operand). As each
//! `SwapEval` fires, it is replaced (higher up the stack) by `Cons` followed
//! by whatever ops the operand's own `eval_pair` call pushes. So at a given
//! point in time, scanning from the top of this frame's region downward:
//!
//! ```text
//! [[ops for the operand currently in flight]] [Cons] [SwapEval]*k [Apply]
//! ```
//!
//! `k` pending `SwapEval`s remain for the `k` lowest-indexed operands not yet
//! started; the in-flight operand's index is exactly `k` (0-based). `Cons` is
//! absent only when either (a) no operand has started yet (frame just
//! entered, about to fire `Apply`) or (b) the in-flight operand already fired
//! `Apply` and dispatched into `apply_op`, which pops its own `Apply`/
//! `val_stack`/`env_stack` entries *before* calling into the operator
//! implementation — so a failure raised from inside an operator (e.g.
//! `DivisionByZero`, `CostExceeded`) or from `apply_kw`'s recursive
//! `eval_pair` leaves **zero residue** for that operand's own frame. See
//! "wrinkles" below.
//!
//! `val_stack`, in lockstep, holds for the same frame:
//! `[operator, operand_0, ..., operand_{k-1} (pending, unevaluated source
//! NodePtrs), accumulator (cons-list of already-evaluated operand values)]`.
//!
//! `env_stack` holds one entry per still-pending `Apply` boundary, pushed
//! immediately before it and popped by `apply_op` at entry (before operator
//! dispatch, i.e. before it can fail) — so its *length* at any point equals
//! the number of `Apply` boundaries still live on `op_stack`, in matching
//! order from oldest (bottom) to newest (top).
//!
//! `ExitGuard` is a frame boundary analogous to `Apply` but for a softfork
//! guard's `eval_pair(prg, env)` call; it does **not** consume `env_stack`
//! itself (an `env_stack` entry only appears if `prg` is itself an
//! operator-call pair, same as any nested `eval_pair`). `RestoreAllocator`
//! and `PostEval` are transparent bookkeeping ops that can appear just below
//! an `Apply` boundary (pushed before it) or above `Cons`; they carry no
//! frame information and are skipped.
//!
//! ## Known wrinkles (raise-site variations)
//!
//! 1. **Operator-dispatch failures leave zero residue for their own frame.**
//!    `apply_op` pops `operand_list`, `operator`, and the `env_stack` entry
//!    *before* calling `dialect.op(...)` (or, for `apply_kw`, before the
//!    recursive `eval_pair`). If that call fails, the failing frame itself
//!    is gone from all three stacks; only the ancestor whose `Cons` is on
//!    top (with nothing above it) survives. The decoder still identifies
//!    *which operand* of that ancestor was in flight (from the pending
//!    `SwapEval` count) and reports that operand's source node as a
//!    synthesized leaf frame — this matches the oracle exactly for the
//!    common case, because the oracle's own unpopped diagnostic frame for
//!    that operand (recorded at `eval_pair` entry, before dispatch) is the
//!    *same* program/env pair.
//! 2. **`apply_kw` breaks the source chain.** `(a P E)` dispatches via
//!    `eval_pair(P_value, E_value)`, where `P_value`/`E_value` are
//!    *evaluated* values, not directly the operands' source nodes. If `P`'s
//!    source is syntactically `(q . X)` (and likewise for `E`), the
//!    evaluated value is textually `X` (quote is an identity passthrough) —
//!    still a genuine source node, just reached via one extra hop. The
//!    decoder recognizes this "apply-of-quote" pattern and chases it
//!    (recursively, for nested applies) using *pure source inspection*, no
//!    stack evidence needed — which is exactly what's required when
//!    `apply_kw`'s dispatch collapses the whole call chain to zero stack
//!    residue (see `ported_nested_apply_frames_zero_residue`, where the
//!    entire failure reconstructs from source alone because there are no
//!    sibling operands anywhere to leave residue).
//! 3. **Apply of a cons-built (non-quote) operator is genuinely opaque.**
//!    If `P` is not syntactically `(q . X)` (e.g. currying via `(c (q . F)
//!    args)`), the dispatched program is a freshly-computed value with no
//!    source position, and by the time we observe the failure the value
//!    itself is usually already gone from every stack too (consumed by
//!    `apply_op`). The decoder reports [`ProgramSource::Opaque`] with
//!    whatever partial evidence remains (normally none at this exact
//!    boundary) rather than guessing.
//! 4. **Malformed operand lists (`InvalidNilTerminator`)** leave `Apply` +
//!    some `SwapEval`s pushed but *not* the trailing accumulator `nil` (the
//!    push loop in `eval_op_atom` raises before reaching it) — one `val`
//!    entry short of the "normal" shape. The decoder treats a short
//!    accumulator read as "no accumulator" rather than panicking.
//!
//! Degrade-gracefully policy: this module never panics and never asserts
//! something it can't verify from what's in front of it. Where the pattern
//! doesn't parse, it stops and returns what it has, plus a `notes` entry
//! explaining why.

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::error::EvalErr;
use crate::traverse_path::traverse_path;

/// A stable, small projection of the evaluator's private `Operation` enum,
/// used so this module doesn't need to depend on `Operation`'s internals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    Apply,
    Cons,
    ExitGuard,
    SwapEval,
    RestoreAllocator,
    /// Anything else (`PostEval`, under the `pre-eval` feature). The
    /// production entry point (`run_program_with_diagnostics`) never enables
    /// `pre_eval`, so this never actually appears in a snapshot it produces;
    /// included so the projection is total and the decoder degrades
    /// gracefully (skips it) rather than panics if that ever changes.
    Other,
}

/// Read-only snapshot of the evaluator's three stacks, bottom (oldest,
/// index 0) to top (newest, last index) — same ordering `RunProgramContext`
/// uses internally.
#[derive(Clone, Debug, Default)]
pub struct StackSnapshot {
    pub op_stack: Vec<OpKind>,
    pub val_stack: Vec<NodePtr>,
    pub env_stack: Vec<NodePtr>,
}

/// How a decoded frame's program was identified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramSource {
    /// The program is a genuine node in the original source tree (possibly
    /// reached through one or more apply-of-quote hops), at `NodePtr`.
    Source(NodePtr),
    /// The program is a computed value with no source position (apply of a
    /// non-quote, e.g. cons-built, operator). `.0` is the best remaining
    /// evidence of the actual value, if any part of it is still visible on
    /// a stack; it is printable but not attributable to a source line.
    Opaque(Option<NodePtr>),
}

/// One decoded frame of the failure trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedFrame {
    /// Nesting depth from the root program (root is depth 0).
    pub depth: usize,
    /// Operand-index path from the root program to this frame, where it is
    /// source-reachable (empty for the root; each element is the operand
    /// index taken at that level). A path through an opaque hop is cut
    /// short: everything *before* the opaque frame is a real path, the
    /// opaque frame and anything after it carry no further path info.
    pub path: Vec<usize>,
    pub program: ProgramSource,
    /// The frame's environment, if resolved. `None` when the frame was
    /// reached via an apply dispatch whose env argument isn't recoverable
    /// without further evaluation (not literally quoted, not a plain atom
    /// path we can resolve ourselves).
    pub environment: Option<NodePtr>,
}

/// The full decode result: frames oldest (root) to newest, plus any notes
/// about where/why the decoder stopped or had to guess.
#[derive(Clone, Debug, Default)]
pub struct DecodedTrace {
    pub frames: Vec<DecodedFrame>,
    pub notes: Vec<String>,
}

/// Decode a post-failure stack snapshot into a failure trace.
///
/// `root_program`/`root_env` are the program/env passed to the top-level
/// `run_program` call (always known to the caller, never ambiguous: by
/// definition, if evaluation failed, the root program was being evaluated).
/// `quote_kw`/`apply_kw` are the dialect's keyword codes (needed to
/// recognize the apply-of-quote source pattern; passed as plain `u32`s so
/// this module doesn't need to be generic over `Dialect`).
///
/// Never panics; degrades to a shorter trace plus a note on anything it
/// can't parse.
pub fn decode_failure_trace(
    allocator: &Allocator,
    quote_kw: u32,
    apply_kw: u32,
    root_program: NodePtr,
    root_env: NodePtr,
    _error: &EvalErr,
    snapshot: &StackSnapshot,
) -> DecodedTrace {
    let mut trace = DecodedTrace::default();

    // `program`/`env` name the *current* frame's identity. `Opaque` means we
    // no longer know a source node for it (apply of a cons-built operator);
    // once a frame is opaque we stop trying to identify anything deeper,
    // since "operand i of an unknown program" isn't a thing we can recover.
    let mut program = ProgramSource::Source(root_program);
    let mut env = root_env;
    let mut env_known = true;
    let mut path: Vec<usize> = Vec::new();
    let mut depth = 0usize;

    let op = &snapshot.op_stack;
    let val = &snapshot.val_stack;
    let envs = &snapshot.env_stack;
    let mut op_i = 0usize;
    let mut val_i = 0usize;
    let mut env_i = 0usize;

    loop {
        trace.frames.push(DecodedFrame {
            depth,
            path: path.clone(),
            program,
            environment: if env_known { Some(env) } else { None },
        });

        let ProgramSource::Source(current_program) = program else {
            // Opaque frame: we have no source identity to walk further
            // operands from. (Its own stack residue, if any, belongs to a
            // call we can't attribute to source, so we stop here rather
            // than guess.)
            break;
        };

        // Structural shortcut, tried *before* consulting the stack at all:
        // `(a (q . X) (q . Y))` is *provably* zero-residue for its own
        // apply-evaluates-its-2-operands frame — both operands are quoted,
        // so both resolve instantly (no nested ops, no pending siblings),
        // and the whole frame retires the moment apply_op dispatches.
        // Whatever is sitting at `op[op_i]` right now therefore cannot
        // belong to *this* frame; it belongs to whatever's deeper (or
        // nothing, if that's zero-residue too). We must hop here rather
        // than attempt stack matching, which would otherwise (wrongly)
        // consume unrelated deeper residue as if it were this frame's own.
        if is_quote_of_quote_apply(allocator, quote_kw, apply_kw, current_program) {
            match apply_hop(
                allocator,
                quote_kw,
                apply_kw,
                current_program,
                env,
                env_known,
            ) {
                Some(hop) => {
                    apply_hop_result(
                        hop,
                        &mut program,
                        &mut env,
                        &mut env_known,
                        &mut path,
                        &mut depth,
                        &mut trace,
                    );
                    continue;
                }
                None => unreachable!("is_quote_of_quote_apply implies apply_hop succeeds"),
            }
        }

        // Skip transparent bookkeeping ops that may sit just below this
        // frame's boundary (RestoreAllocator is pushed before push_env/Apply
        // when the operator is a GC candidate).
        while op_i < op.len() && matches!(op[op_i], OpKind::RestoreAllocator | OpKind::Other) {
            op_i += 1;
        }

        if op_i >= op.len() {
            // This frame's own Apply boundary already fired (zero residue)
            // or was never pushed (quote/atom-path root). Only pure source
            // reasoning (apply-of-quote chasing) can go deeper from here —
            // and if it succeeds, the dispatched child's *own* fresh ops (if
            // any) begin exactly at this same cursor position, since
            // nothing else was pushed in between. So on success we loop
            // back and keep consuming the stack normally for the new frame.
            match apply_hop(
                allocator,
                quote_kw,
                apply_kw,
                current_program,
                env,
                env_known,
            ) {
                Some(hop) => {
                    apply_hop_result(
                        hop,
                        &mut program,
                        &mut env,
                        &mut env_known,
                        &mut path,
                        &mut depth,
                        &mut trace,
                    );
                    continue;
                }
                None => break,
            }
        }

        match op[op_i] {
            OpKind::Apply | OpKind::ExitGuard => {
                let is_apply = op[op_i] == OpKind::Apply;

                // Decisive sanity check, *before* consuming anything: an
                // apply_kw frame whose own P/E happen to evaluate (at
                // runtime) without failing retires with zero residue just
                // like the provably-quoted case above — we just can't tell
                // *in advance* from source alone (P might be a real
                // expression that could have failed, it just didn't this
                // time). If so, whatever's sitting at `op[op_i]` belongs to
                // the *dispatched* frame, not this one. We tell the two
                // apart by checking whether val_stack's operator slot holds
                // this frame's own operator atom NodePtr (literally
                // `program`'s op_node, pushed verbatim by `eval_op_atom`) —
                // a precise identity check, unlike comparing envs, which can
                // coincidentally match (e.g. both frames happen to use
                // `nil`). Back off (consume nothing) and hop instead, rather
                // than mis-attribute a dispatched frame's residue to this
                // one.
                if is_apply
                    && is_apply_dispatch(allocator, apply_kw, current_program)
                    && val.get(val_i) != Some(&apply_operator_node(allocator, current_program))
                {
                    match apply_hop(
                        allocator,
                        quote_kw,
                        apply_kw,
                        current_program,
                        env,
                        env_known,
                    ) {
                        Some(hop) => {
                            apply_hop_result(
                                hop,
                                &mut program,
                                &mut env,
                                &mut env_known,
                                &mut path,
                                &mut depth,
                                &mut trace,
                            );
                            continue;
                        }
                        None => {
                            trace.notes.push(format!(
                                "depth {depth}: env_stack[{env_i}] didn't match this apply_kw \
                                 frame's expected env, and it isn't a recoverable apply-of-quote \
                                 either; stopping rather than guess"
                            ));
                            break;
                        }
                    }
                }

                op_i += 1;

                if is_apply {
                    // Sanity-check against env_stack where possible; don't
                    // panic if it doesn't line up (e.g. malformed-input or
                    // an unaccounted-for wrinkle) — just note it. (The
                    // apply_kw case that's actually a sign of a collapsed
                    // ancestor is handled above, before we get here.)
                    if env_known {
                        match envs.get(env_i) {
                            Some(&e) if e == env => {}
                            Some(_) => trace.notes.push(format!(
                                "depth {depth}: env_stack[{env_i}] didn't match the frame's \
                                 expected env; continuing without re-deriving it"
                            )),
                            None => trace.notes.push(format!(
                                "depth {depth}: expected an env_stack entry at index {env_i} \
                                 for this Apply boundary but env_stack was shorter"
                            )),
                        }
                    }
                    env_i += 1;
                }

                // Also account for the operator + accumulator val_stack
                // entries this frame owns, so val_i tracks correctly for
                // any sanity-checking we add later. We don't strictly need
                // the operator value itself here (the frame's program is
                // already known), but step past it if present.
                if val_i < val.len() {
                    val_i += 1; // operator
                }

                // Count pending SwapEvals (operands not yet started).
                let mut k = 0usize;
                while op_i < op.len() && op[op_i] == OpKind::SwapEval {
                    op_i += 1;
                    k += 1;
                }
                val_i += k; // pending operand programs

                if op_i < op.len() && op[op_i] == OpKind::Cons {
                    op_i += 1;
                    if val_i < val.len() {
                        val_i += 1; // accumulator
                    }

                    match nth_operand(allocator, current_program, k) {
                        Some(child_program) => {
                            // Cross-check against val_stack's recorded
                            // pending-operand NodePtrs where available: the
                            // operand at index k is the one *in flight*, so
                            // it won't be sitting in val_stack as a pending
                            // entry (only indices 0..k are pending); nothing
                            // to check here directly, but k itself came from
                            // counting SwapEvals, independent of val_stack.
                            path.push(k);
                            depth += 1;
                            program = ProgramSource::Source(child_program);
                            // Same env flows down to every operand.
                            // env/env_known unchanged.
                        }
                        None => {
                            trace.notes.push(format!(
                                "depth {depth}: Cons observed with {k} pending SwapEvals, but \
                                 the frame's program doesn't have an operand at index {k}; \
                                 stopping"
                            ));
                            break;
                        }
                    }
                } else {
                    // No Cons: this frame's own Apply is about to fire (0
                    // residue). Same apply-of-quote chase as above.
                    match apply_hop(
                        allocator,
                        quote_kw,
                        apply_kw,
                        current_program,
                        env,
                        env_known,
                    ) {
                        Some(hop) => {
                            apply_hop_result(
                                hop,
                                &mut program,
                                &mut env,
                                &mut env_known,
                                &mut path,
                                &mut depth,
                                &mut trace,
                            );
                        }
                        None => break,
                    }
                }
            }
            other => {
                trace.notes.push(format!(
                    "depth {depth}: expected an Apply/ExitGuard boundary but found {other:?} \
                     at op_stack[{op_i}]; stack pattern didn't parse, stopping"
                ));
                break;
            }
        }
    }

    trace
}

/// Outcome of one apply-of-quote hop attempt.
enum ApplyHop {
    Resolved {
        program: NodePtr,
        env: NodePtr,
        env_known: bool,
    },
    Opaque,
}

/// Apply `ApplyHop` to the decode loop's running state, pushing a new frame
/// for the dispatched child.
fn apply_hop_result(
    hop: ApplyHop,
    program: &mut ProgramSource,
    env: &mut NodePtr,
    env_known: &mut bool,
    path: &mut Vec<usize>,
    depth: &mut usize,
    trace: &mut DecodedTrace,
) {
    *depth += 1;
    // Path accounting stops making sense once we've hopped through an apply
    // dispatch (the child isn't "operand i of program" in the ordinary
    // sense) — reset it rather than claim a path that doesn't correspond to
    // a real traversal from the root.
    path.clear();
    match hop {
        ApplyHop::Resolved {
            program: p,
            env: e,
            env_known: ek,
        } => {
            if !ek {
                trace.notes.push(
                    "apply dispatch's env argument isn't literally quoted or a resolvable \
                     atom path — env unresolved for this frame and below"
                        .to_string(),
                );
            }
            *program = ProgramSource::Source(p);
            *env = e;
            *env_known = ek;
        }
        ApplyHop::Opaque => {
            trace.notes.push(
                "apply-of-non-quote operator: dispatch target has no source position (opaque, \
                 currying/cons-built case) — stopping source descent"
                    .to_string(),
            );
            *program = ProgramSource::Opaque(None);
            *env_known = false;
        }
    }
}

/// Try one apply-of-quote hop from `program` (which has already run out of
/// stack residue of its own): if `program` is syntactically `(a P E)`,
/// return the dispatched child's identity — `Source` if `P` is literally
/// `(q . X)` (quote is an identity passthrough, so `X` is a genuine source
/// node), `Opaque` if `P` is anything else (a cons-built/computed operator,
/// with no source position and — by the time we can observe a failure —
/// usually no remaining value either, since `apply_op` has already popped
/// it). Returns `None` if `program` isn't an apply dispatch at all (nothing
/// more to chase; this frame's own operator call is where the detail, if
/// any, lives — in the `EvalErr`, not in another frame).
fn apply_hop(
    allocator: &Allocator,
    quote_kw: u32,
    apply_kw: u32,
    program: NodePtr,
    env: NodePtr,
    env_known: bool,
) -> Option<ApplyHop> {
    let SExp::Pair(op_node, _op_list) = allocator.sexp(program) else {
        // Atom program (env-path lookup) with no further ops pushed: this is
        // the atom-leaf-failure wrinkle. Nothing deeper to chase.
        return None;
    };
    let SExp::Atom = allocator.sexp(op_node) else {
        // `((X)...)` special form, or anything else with a pair head: no
        // further source-level descent defined for this shape.
        return None;
    };
    if allocator.small_number(op_node) != Some(apply_kw) {
        // Not an apply dispatch: this frame's own operator call is whatever
        // failed (e.g. inside dialect.op), and there's no deeper *source*
        // frame to synthesize.
        return None;
    }

    // `(a P E)` — exactly 2 operands, enforced by apply_op's own
    // get_args::<2>. If malformed, nothing to chase.
    let p_node = nth_operand(allocator, program, 0)?;
    let e_node = nth_operand(allocator, program, 1)?;

    let Some(new_program) = quoted_value(allocator, quote_kw, p_node) else {
        // Apply of a non-quote (e.g. cons-built) operator: the dispatched
        // program is a computed value with no source position, and
        // apply_op has already popped it from every stack by the time
        // we'd observe a failure here.
        return Some(ApplyHop::Opaque);
    };

    let (new_env, new_env_known) = match quoted_value(allocator, quote_kw, e_node) {
        Some(x) => (x, true),
        None if env_known => {
            // Not literally quoted, but if it's an atom (plain env path) we
            // can resolve it ourselves: traverse_path is a pure, total
            // function of (path, env), nothing to re-run.
            if let SExp::Atom = allocator.sexp(e_node) {
                match traverse_path(allocator, allocator.atom(e_node).as_ref(), env) {
                    Ok(reduction) => (reduction.1, true),
                    Err(_) => (NodePtr::NIL, false),
                }
            } else {
                (NodePtr::NIL, false)
            }
        }
        None => (NodePtr::NIL, false),
    };

    Some(ApplyHop::Resolved {
        program: new_program,
        env: new_env,
        env_known: new_env_known,
    })
}

/// Get the `index`-th operand (0-based) of `program`'s operand list, by pure
/// source inspection: `program` must be `(operator . operand_list)` and
/// `operand_list` a proper list of at least `index + 1` elements.
fn nth_operand(allocator: &Allocator, program: NodePtr, index: usize) -> Option<NodePtr> {
    let SExp::Pair(_op_node, mut operands) = allocator.sexp(program) else {
        return None;
    };
    for _ in 0..index {
        let SExp::Pair(_, rest) = allocator.sexp(operands) else {
            return None;
        };
        operands = rest;
    }
    let SExp::Pair(first, _) = allocator.sexp(operands) else {
        return None;
    };
    Some(first)
}

/// `program`'s operator-position `NodePtr` (the `op_node` `eval_op_atom`
/// pushes verbatim onto `val_stack` at frame entry), or `NodePtr::NIL` if
/// `program` isn't even a pair (shouldn't be called in that case).
fn apply_operator_node(allocator: &Allocator, program: NodePtr) -> NodePtr {
    match allocator.sexp(program) {
        SExp::Pair(op_node, _) => op_node,
        SExp::Atom => NodePtr::NIL,
    }
}

/// Is `program`'s operator literally `apply_kw` (i.e. is this frame's own
/// evaluation `(a P E)`, where `apply_op` will, once P/E are both done,
/// dispatch into a *computed* child rather than just consing the result)?
fn is_apply_dispatch(allocator: &Allocator, apply_kw: u32, program: NodePtr) -> bool {
    let SExp::Pair(op_node, _) = allocator.sexp(program) else {
        return false;
    };
    allocator.small_number(op_node) == Some(apply_kw)
}

/// Is `program` syntactically `(a (q . X) (q . Y))`? Both operands being
/// literal quotes is what makes this frame's evaluation *provably*
/// zero-residue (see call site) — unlike every other apply-of-quote
/// variant, where only `P` needs to be quoted and `E` may be a real
/// expression (possibly leaving genuine stack residue while it evaluates).
fn is_quote_of_quote_apply(
    allocator: &Allocator,
    quote_kw: u32,
    apply_kw: u32,
    program: NodePtr,
) -> bool {
    let SExp::Pair(op_node, _) = allocator.sexp(program) else {
        return false;
    };
    if allocator.small_number(op_node) != Some(apply_kw) {
        return false;
    }
    let Some(p_node) = nth_operand(allocator, program, 0) else {
        return false;
    };
    let Some(e_node) = nth_operand(allocator, program, 1) else {
        return false;
    };
    quoted_value(allocator, quote_kw, p_node).is_some()
        && quoted_value(allocator, quote_kw, e_node).is_some()
}

/// If `node` is syntactically `(quote_kw . X)`, return `X`.
fn quoted_value(allocator: &Allocator, quote_kw: u32, node: NodePtr) -> Option<NodePtr> {
    let SExp::Pair(head, rest) = allocator.sexp(node) else {
        return None;
    };
    if allocator.small_number(head) == Some(quote_kw) {
        Some(rest)
    } else {
        None
    }
}
