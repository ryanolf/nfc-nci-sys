extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    if let Ok(lib_dir) = env::var("NFC_NCI_LINUX_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_dir);
    }
	println!("cargo:rustc-link-lib=dylib=nfc_nci_linux");

    // Tell cargo to invalidate the built crate whenever the wrapper changes
    println!("cargo:rerun-if-changed=wrapper.h");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
        .derive_default(true);
    
    let bindings = (
        if let Ok(include_dir) = env::var("NFC_NCI_LINUX_INCLUDE_DIR") {
            println!("cargo:include={}", include_dir);
            bindings.clang_arg(format!("-I/{}", include_dir))
        } else {
            bindings
        })
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}