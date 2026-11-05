#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[tauri::command]
fn public_cad_token() -> Result<String, String> {
    std::env::var("CLASSCAD_TOKEN")
        .ok()
        .filter(|token| token.starts_with("ccpk_"))
        .ok_or_else(|| "Set a public CLASSCAD_TOKEN in the app launch environment. The token must allow this app's origin.".into())
}

fn main() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![public_cad_token]);
    #[cfg(not(dev))]
    let builder = {
        use tauri::{ipc::CapabilityBuilder, Manager, WebviewUrl, WebviewWindowBuilder};
        let port = portpicker::pick_unused_port().expect("No available loopback port");
        builder
            .plugin(tauri_plugin_localhost::Builder::new(port).host("127.0.0.1").build())
            .setup(move |app| {
                let url: tauri::Url = format!("http://localhost:{port}/hardware.html").parse()?;
                app.add_capability(CapabilityBuilder::new("desktop-cad")
                    .remote(format!("http://localhost:{port}/*"))
                    .window("main")
                    .permission("core:default")
                    .permission("allow-public-cad-token")
                    .permission("dialog:allow-save")
                    .permission("fs:allow-write-file"))?;
                let origin = url.origin();
                WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                    .title("Vibestar Hardware").inner_size(1440.0, 1000.0).min_inner_size(390.0, 600.0)
                    .on_navigation(move |url| url.origin() == origin)
                    .build()?;
                Ok(())
            })
    };
    #[cfg(dev)]
    let builder = builder.setup(|app| {
        tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("hardware.html".into()))
            .title("Vibestar Hardware").inner_size(1440.0, 1000.0).min_inner_size(390.0, 600.0).build()?;
        Ok(())
    });
    builder
        .run(tauri::generate_context!())
        .expect("Unable to launch Vibestar hardware workbench");
}
