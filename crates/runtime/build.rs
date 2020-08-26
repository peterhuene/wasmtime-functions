fn main() {
    // TODO: remove this with a new wiggle release
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-env=WASI_ROOT={}", root);
}
