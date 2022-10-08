const wasm = require("../pkg/clvm_wasm.js");

function expect_equal(challenge, expected){
    if(challenge !== expected){
        throw new Error(`Assertion Error: Expected "${expected}" but actual value was "${challenge}"`);
    }
}

function expect_throw(callback){
    let is_error = undefined;
    try{
        callback();
    }
    catch(e){
        is_error = e;
    }

    if(!is_error){
        throw new Error('Expected an exception but it was not thrown');
    }
}

// (q . 127)
let prog1 = Uint8Array.from(Buffer.from("ff017f", "hex"));
// ()
let arg1 = Uint8Array.from(Buffer.from("80", "hex"));
// 100,000,000,000
let max_cost1 = BigInt("100000000000");
let flag1 = 0;
let [cost1, sexp1] = wasm.run_chia_program(prog1, arg1, max_cost1, flag1);
expect_equal(sexp1.atom.toString(), "127");

// (+ 1 (q . 3))
let prog2 = Uint8Array.from(Buffer.from("ff10ff01ffff010380", "hex"));
// 2
let arg2 = Uint8Array.from(Buffer.from("02", "hex"));;
// 100,000,000,000
let max_cost2 = BigInt("100000000000");
let flag2 = 0;
let [cost2, sexp2] = wasm.run_chia_program(prog2, arg2, max_cost2, flag2);
expect_equal(sexp2.atom.toString(), "5");

// (q . 147)
let prog3 = Uint8Array.from(Buffer.from("ff017f", "hex"));
// ()
let arg3 = Uint8Array.from(Buffer.from("80", "hex"));
let max_cost3 = BigInt("1");
let flag3 = 0;
expect_throw(function(){
    wasm.run_chia_program(prog3, arg3, max_cost3, flag3);
});

// (/ (q . 5) (q . -3))
let prog4 = Uint8Array.from(Buffer.from("ff13ffff0105ffff0181fd80", "hex"));
// ()
let arg4 = Uint8Array.from(Buffer.from("80", "hex"));
// 100,000,000,000
let max_cost4 = BigInt("100000000000");
let flag4 = 0;
let [cost4, sexp4] = wasm.run_chia_program(prog4, arg4, max_cost4, flag4);
expect_equal(sexp4.atom.toString(), Uint8Array.from([-2]).toString());
let flag5 = wasm.Flag.no_neg_div();
expect_throw(function(){
    wasm.run_chia_program(prog4, arg4, max_cost4, flag5);
});
