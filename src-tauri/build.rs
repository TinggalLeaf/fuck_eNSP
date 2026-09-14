fn main() {
    // eNSP itself runs elevated on most setups; injecting into it (or
    // launching it) needs matching privileges — embed an admin manifest.
    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new()
                .app_manifest(include_str!("windows-app-manifest.xml")),
        ),
    )
    .expect("tauri build failed");

    // Build the 32-bit native components (hook DLL + injector) and stage them
    // next to this crate's build output so the app can embed them.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let workspace_target =
        std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| format!("{}\\target", manifest_dir));

    let native_dir = std::path::Path::new(&manifest_dir).join("native");
    let stage = std::path::Path::new(&workspace_target)
        .join("native-stage")
        .join(&profile);
    std::fs::create_dir_all(&stage).unwrap();

    for crate_name in ["hook_dll", "inject_helper"] {
        let dir = native_dir.join(crate_name);
        println!("cargo:rerun-if-changed={}", dir.join("src").display());
        println!("cargo:rerun-if-changed={}", dir.join("Cargo.toml").display());

        let mut args: Vec<String> = vec![
            "build".into(),
            "--target".into(),
            "i686-pc-windows-msvc".into(),
        ];
        if profile == "release" {
            args.push("--release".into());
        }
        args.push("--manifest-path".into());
        let manifest = dir.join("Cargo.toml");
        args.push(manifest.to_str().unwrap().to_string());
        let status = std::process::Command::new("cargo")
            .args(&args)
            .env(
                "CARGO_TARGET_DIR",
                std::path::Path::new(&workspace_target).join("native-i686"),
            )
            .status()
            .expect("failed to run cargo for native component");
        if !status.success() {
            panic!("native component {} failed to build", crate_name);
        }
    }

    let bin_dir = std::path::Path::new(&workspace_target)
        .join("native-i686")
        .join("i686-pc-windows-msvc")
        .join(&profile);
    for f in ["fuck_ensp_hook.dll", "fuck_inject32.exe"] {
        std::fs::copy(bin_dir.join(f), stage.join(f)).unwrap_or_else(|e| {
            panic!("missing native artifact {}: {}", f, e);
        });
    }
    println!("cargo:rustc-env=NATIVE_STAGE_DIR={}", stage.display());
}
