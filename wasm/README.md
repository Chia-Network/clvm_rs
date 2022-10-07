The `clvm_rs` package has JavaScript bindings for the rust implementation of clvm in wasm.

This project is very immature, and only some test API is available for the moment. Pull requests are welcome.


Build
-----

Use `wasm-pack` to build the wasm `pkg` file used with npm. Install it with:

```
$ cargo install wasm-pack
```

Then build with

```
$ wasm-pack build --release --target=nodejs
```
