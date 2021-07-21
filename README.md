Rust implementation of clvm.


Install into current virtualenv with

```
$ pip install maturin
$ maturin develop --release --cargo-extra-args='--features extension-module,openssl'  # on Windows, drop `openssl`
$ pip install git+https://github.com/Chia-Network/clvm@use_clvm_rs
```

Note that for now, you must use the `use_clvm_rs` branch of `clvm`.

The rust code replaces `run_program` and `CLVMObject`.

In order to run the unit tests, run:

```
cargo test
```


To build a wasm `pkg`, use
```
$ cargo install wasm-pack
$ wasm-pack build  --release -- --features=wasm-api
```
