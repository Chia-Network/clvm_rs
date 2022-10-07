const wasm = require("../pkg/clvm_wasm.js");

function assert(value){
    if(!value){
        throw new Error("assertion error");
    }
}

// (q . 127)
let prog1 = Uint8Array.from(Buffer.from("ff017f", "hex"));
// ()
let arg1 = Uint8Array.from(Buffer.from("00", "hex"));
let [cost1, sexp1] = wasm.run_chia_program(prog1, arg1, BigInt("100000000000"), 0);

assert(sexp1.atom.toString() === "127");

// (+ 1 (q . 3))
let prog2 = Uint8Array.from(Buffer.from("ff10ff01ffff010380", "hex"));
// 2
let arg2 = Uint8Array.from(Buffer.from("02", "hex"));;
let [cost2, sexp2] = wasm.run_chia_program(prog2, arg2, BigInt("100000000000"), 0);

// assert(sexp2.atom.toString() === "5");
assert(sexp2.atom.toString() === "xxxx"); // Make it fail to check whether CI action actually fails.
