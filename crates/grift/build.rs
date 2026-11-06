fn main() -> Result<(), std::env::VarError> {
    // The unoptimized embedding test must use the compiler Cargo selected for this build.
    println!(
        "cargo:rustc-env=GRIFT_TEST_RUSTC={}",
        std::env::var("RUSTC")?
    );
    println!("cargo:rerun-if-env-changed=RUSTC");
    Ok(())
}
