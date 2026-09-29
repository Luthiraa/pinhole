fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("macos") {
        return;
    }
    cc::Build::new()
        .file("src/host/capture.m")
        .flag("-fobjc-arc")
        .flag("-mmacosx-version-min=12.3")
        .compile("pinhole_capture");
    println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-lib=framework=CoreVideo");
    println!("cargo:rustc-link-lib=framework=ImageIO");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rerun-if-changed=src/host/capture.m");
}
