//! Print detailed info about the first connected XR50 device.

fn main() {
    env_logger::init();

    match xvisio::Device::open_first() {
        Ok(device) => {
            println!("UUID:     {}", device.uuid());
            println!("Version:  {}", device.version());
            println!("Features: {:?}", device.features());
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
