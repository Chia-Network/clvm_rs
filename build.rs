fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-lib=mpir");
        println!("cargo:rustc-link-search=mpir_gc_x64");
    }
    #[cfg(linux)]
    {
        println!("cargo:rustc-link-lib=libgmp.so.3");
        println!("cargo:rustc-link-search=/usr/lib64");
    }
    #[cfg(macos)]
    {
        println!("cargo:rustc-link-lib=gmp");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
    }
}
