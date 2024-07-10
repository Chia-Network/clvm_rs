The `clvm_rs` wheel has python bindings for the rust implementation of clvm.

Build
-----

Use `maturin` to build the python interface. First, install into current virtualenv with

```
$ pip install maturin
```

While in the `wheel` directory, build `clvm_rs` into the current virtualenv with

```
$ maturin develop --release
```

To build the wheel, do

```
$ maturin build --release
````
