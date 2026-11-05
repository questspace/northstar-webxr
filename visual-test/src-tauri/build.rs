fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["public_cad_token"]),
    )).expect("Unable to generate application permissions")
}
