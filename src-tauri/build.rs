fn main() {
    println!("cargo:rerun-if-env-changed=DF_TAURI_CLI_BUILD");
    println!("cargo:rerun-if-env-changed=DF_ALLOW_RAW_CARGO_RELEASE");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let tauri_cli_build = std::env::var_os("DF_TAURI_CLI_BUILD").is_some();
    let raw_release_override = std::env::var_os("DF_ALLOW_RAW_CARGO_RELEASE").is_some();

    if target_os == "windows" && profile == "release" && !tauri_cli_build && !raw_release_override {
        panic!(
            "Refusing a raw Windows Cargo release build. This can produce a desktop binary that loads the dev URL (localhost:1430). Build production Windows artifacts through `npm run tauri -- build` instead. For an intentional low-level Cargo-only diagnostic build, set DF_ALLOW_RAW_CARGO_RELEASE=1; never promote that artifact as the desktop release."
        );
    }

    tauri_build::build()
}
