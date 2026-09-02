fn main() {
    println!("cargo:rerun-if-changed=memory.x");
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    // Feature is only used for `cargo test --features on-target --test on_target`.
    if std::env::var("CARGO_FEATURE_ON_TARGET").is_ok() {
        println!("cargo:rustc-link-arg=-Tembedded-test.x");
    }
}
