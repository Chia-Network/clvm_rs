const wasm = require("../pkg/clvm_wasm.js");

function expect_equal(challenge, expected) {
  if (challenge !== expected) {
    throw new Error(
      `Assertion Error: Expected "${expected}" but actual value was "${challenge}"`,
    );
  }
}

function expect_throw(callback) {
  let is_error = undefined;
  try {
    callback();
  } catch (e) {
    is_error = e;
  }

  if (!is_error) {
    throw new Error("Expected an exception but it was not thrown");
  }
}

function bytesFromHex(hex) {
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

function numsToByteStr(numArray) {
  return Uint8Array.from(numArray).toString();
}

let current_test_number = 0;
function test_case(testTitle, test) {
  const testNo = ++current_test_number;
  console.log(`Case#${testNo} ${testTitle}`);
  try {
    test();
    console.log(`✓ Successfully finished case#${testNo}`);
  } catch (e) {
    console.error(`❌ Failed Case#${testNo}`);
    console.error(`${e.name}: ${e.message}`);
    process.exit(1);
  }
}

// ----------------------------------------------------- //

test_case("Test '(q . 127)' '()'", function () {
  // (q . 127)
  const prog = bytesFromHex("ff017f");
  // ()
  const arg = bytesFromHex("80");
  // 100,000,000,000
  const max_cost = BigInt("100000000000");
  const flag = 0;
  const [cost, sexp] = wasm.run_chia_program(prog, arg, max_cost, flag);
  expect_equal(sexp.atom.toString(), "127");
});

test_case("Test '(+ 1 (q . 3))' '2'", function () {
  // (+ 1 (q . 3))
  const prog = bytesFromHex("ff10ff01ffff010380");
  // 2
  const arg = bytesFromHex("02");
  // 100,000,000,000
  const max_cost = BigInt("100000000000");
  const flag = 0;
  const [cost, sexp] = wasm.run_chia_program(prog, arg, max_cost, flag);
  expect_equal(sexp.atom.toString(), "5");
});

test_case("Test '(+ 7 (q . 3))' '(() . (() . 2))'", function () {
  // (+ 7 (q . 3))
  const prog = bytesFromHex("ff10ff07ffff010380");
  // (() . (() . 2))
  const arg = bytesFromHex("ff80ff8002");
  // 100,000,000,000
  const max_cost = BigInt("100000000000");
  const flag = 0;
  const [cost, sexp] = wasm.run_chia_program(prog, arg, max_cost, flag);
  expect_equal(sexp.atom.toString(), "5");
});

test_case("Test max_cost too low", function () {
  // (q . 127)
  const prog = bytesFromHex("ff017f");
  // ()
  const arg = bytesFromHex("80");
  // MaxCost too low
  const max_cost = BigInt("1");
  const flag = 0;
  expect_throw(function () {
    wasm.run_chia_program(prog, arg, max_cost, flag);
  });
});

test_case("Test divmod", function () {
  // (divmod (q . 5) (q . -3))
  const prog = bytesFromHex("ff14ffff0105ffff0181fd80");
  // ()
  const arg = bytesFromHex("80");
  // 100,000,000,000
  const max_cost = BigInt("100000000000");
  const flag = 0;
  const [cost, sexp] = wasm.run_chia_program(prog, arg, max_cost, flag);
  expect_equal(sexp.pair[0].atom.toString(), numsToByteStr([-2]));
  expect_equal(sexp.pair[1].atom.toString(), numsToByteStr([-1]));
});

test_case("Test negative div", function () {
  // (/ (q . 5) (q . -3))
  const prog = bytesFromHex("ff13ffff0105ffff0181fd80");
  // ()
  const arg = bytesFromHex("80");
  // 100,000,000,000
  const max_cost = BigInt("100000000000");
  const [cost, sexp] = wasm.run_chia_program(prog, arg, max_cost, 0);
  // div rounds towards negative infinity, so this is -2
  expect_equal(sexp.atom.toString(), "254");
});

test_case("Test serialized_length", function () {
  // (q . 127)
  const prog = bytesFromHex("ff017f");
  expect_equal(wasm.serialized_length(prog), BigInt("3"));
  expect_throw(function () {
    wasm.serialized_length(bytesFromHex("abcdef0123"));
  });
  try {
    wasm.serialized_length(bytesFromHex("abcdef0123"));
  } catch (e) {
    expect_equal(e, "bad encoding");
  }
});
