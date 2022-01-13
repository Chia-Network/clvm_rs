use std::env;

fn main() {
    if env::var_os("CARGO_FEATURE_MPIR").is_some() {
        println!("cargo:rustc-link-lib=mpir");
        println!("cargo:rustc-link-search=mpir_gc_x64");
    } else if env::var_os("CARGO_FEATURE_LIBGMP3").is_some() {
        println!("cargo:rustc-link-lib=libgmp.so.3");
    } else if env::var_os("CARGO_FEATURE_LIBGMP10").is_some() {
        println!("cargo:rustc-link-lib=libgmp.so.10");
    } else if env::var_os("CARGO_FEATURE_LIBGMP").is_some() {
        println!("cargo:rustc-link-lib=gmp");
    }

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-search=/usr/lib64");
        println!("cargo:rustc-link-search=/usr/lib");
    }
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
    }
}
