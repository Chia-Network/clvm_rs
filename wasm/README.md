The `clvm_rs` package has JavaScript bindings for the rust implementation of clvm in wasm.  
This project is very immature, and only some test API is available for the moment. Pull requests are welcome.

## Build

Use `wasm-pack` to build the wasm `pkg` file used with npm. Install it with:

```bash
cargo install wasm-pack
```

Then build with

```bash
# Make sure you're at <clvm_rs root>/wasm
wasm-pack build --release --target=nodejs
```

## Test

Prerequisite:

- NodeJS >= 16
- Wasm files built by `wasm-pack` command exist at `<clvm_rs root>/wasm/pkg/`

```bash
# Make sure you're at <clvm_rs root>/wasm
node ./tests/index.js
```
