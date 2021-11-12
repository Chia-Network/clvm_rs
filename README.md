Rust implementation of clvm.

Python Wheel
------------

Use `maturin` to build the python interface. First, install into current virtualenv with

```
$ pip install maturin
```

Build `clvm_rs` directly into the current virtualenv with

```
$ maturin develop --release
```

On UNIX-based platforms, you may get a speed boost on `sha256` operations by building
with OpenSSL.

```
$ maturin develop --release --cargo-extra-args="--features=openssl"
```


To build the wheel, do

```
$ maturin build --release --no-sdist
````

or

```
$ maturin build --release --no-sdist --cargo-extra-args="--features=openssl"
```


WASM
----

Use `wasm-pack` to build the wasm `pkg` file used with npm. Install it with:

```
$ cargo install wasm-pack
```

Then build with

```
$ wasm-pack build --release
```


TESTS
-----
In order to run the unit tests, run:

```
cargo test
```
