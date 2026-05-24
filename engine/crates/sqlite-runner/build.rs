fn main() {
    if std::env::var_os("CARGO_FEATURE_NATIVE").is_some() {
        println!("cargo:rustc-link-lib=sqlite3");
    }
}
