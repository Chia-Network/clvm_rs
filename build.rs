fn main() {
    if cfg!(windows) {
        println!("cargo:rustc-link-lib=mpir");
        println!("cargo:rustc-link-search=mpir_gc_x64");
    } else {
        println!("cargo:rustc-link-lib=gmp");
        println!("cargo:rustc-link-search=/usr/local/lib");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
    }
}
