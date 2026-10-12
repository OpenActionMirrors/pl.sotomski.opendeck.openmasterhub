use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let profile_dir = out
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is target/<profile>/build/<pkg>/out");
    let bin = profile_dir.join("opendeck-masterhub");
    println!("cargo:rustc-env=PLUGIN_BIN={}", bin.display());
}
