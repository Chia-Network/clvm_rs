fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-lib=mpir");
        println!("cargo:rustc-link-search=mpir_gc_x64");
    }
    #[cfg(not(windows))]
    {
        println!("cargo:rustc-link-lib=gmp");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
        println!("cargo:rustc-link-search=/usr/lib64");
    }
}
