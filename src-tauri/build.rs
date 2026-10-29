fn main() {
    link_clang_rt_on_macos();
    tauri_build::build()
}

/// ggml-metal's Objective-C uses `@available(macOS 14, *)` checks, which clang
/// lowers to calls into `__isPlatformVersionAtLeast` — a builtin that lives in
/// compiler-rt (`libclang_rt.osx.a`). rustc links with `-nodefaultlibs`, so
/// that archive is never pulled in and the final link fails with an undefined
/// symbol. Pass the archive explicitly. It only adds the objects that are
/// actually referenced, so it's harmless when Metal is off (Intel build).
fn link_clang_rt_on_macos() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::process::Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
        .or_else(|_| {
            std::process::Command::new("clang")
                .arg("--print-resource-dir")
                .output()
        });
    let Ok(out) = out else {
        println!("cargo:warning=could not run clang --print-resource-dir; Metal link may fail");
        return;
    };
    let resource_dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let lib = std::path::Path::new(&resource_dir)
        .join("lib")
        .join("darwin")
        .join("libclang_rt.osx.a");
    if lib.exists() {
        println!("cargo:rustc-link-arg={}", lib.display());
    } else {
        println!(
            "cargo:warning=libclang_rt.osx.a not found at {}; Metal link may fail",
            lib.display()
        );
    }
}
