//! Links LeapC from the installed Ultraleap SDK and drops LeapC.dll next to the binary.
use std::path::PathBuf;

fn main() {
    let sdk = PathBuf::from(std::env::var("LEAPSDK_DIR").unwrap_or_else(|_| {
        r"C:\Program Files\Ultraleap\LeapSDK".into()
    }));
    let lib = sdk.join("lib").join("x64");
    println!("cargo:rerun-if-env-changed=LEAPSDK_DIR");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=dylib=LeapC");

    // OUT_DIR = target/<profile>/build/<pkg>/out → target/<profile>
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if let Some(target) = out.ancestors().nth(3) {
        let _ = std::fs::copy(lib.join("LeapC.dll"), target.join("LeapC.dll"));
    }
}
