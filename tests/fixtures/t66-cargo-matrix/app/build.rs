use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    fs::write(
        output.join("matrix-generated.rs"),
        "pub const GENERATED_VALUE: &str = \"generated\";\n",
    )
    .expect("write generated Rust source");
    println!("cargo:rerun-if-changed=build.rs");
}
