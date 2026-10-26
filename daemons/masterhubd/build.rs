fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    cc::Build::new()
        .file("src/usb/set_idle_macos.m")
        .flag("-fobjc-arc")
        .compile("mh_set_idle");
    println!("cargo:rustc-link-lib=framework=IOUSBHost");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=IOKit");
}
