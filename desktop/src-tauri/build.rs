fn main() {
    // The sidecar name carries the target triple; keep it available to the
    // binary without hard-coding the host platform.
    println!(
        "cargo:rustc-env=HN_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    tauri_build::build()
}
