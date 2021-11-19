fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=mpir");
        println!("cargo:rustc-link-search=mpir_gc_x64");
    }
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=gmp");
        println!("cargo:rustc-link-search=/usr/lib64");
        println!("cargo:rustc-link-search=/usr/lib");
    }
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=gmp");
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
    }
}
