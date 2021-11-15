fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-lib=mpir");
        println!("cargo:rustc-link-search=mpir_gc_x64");
    }
}
