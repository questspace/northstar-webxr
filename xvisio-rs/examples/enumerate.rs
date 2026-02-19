//! List all connected XVisio XR50 devices.

fn main() {
    env_logger::init();

    match xvisio::device::list_devices() {
        Ok(devices) => {
            println!("Found {} XR50 device(s):", devices.len());
            for (i, dev) in devices.iter().enumerate() {
                println!(
                    "  [{}] UUID={}  FW={}  Features={:?}  Bus={} Addr={}",
                    i, dev.uuid, dev.version, dev.features, dev.bus_id, dev.device_address
                );
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
