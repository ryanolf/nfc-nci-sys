extern crate bindgen;

use std::env;
use std::path::PathBuf;

#[cfg(not(feature = "vendored"))]
mod lib {
    use std::env;

    pub fn get_builder() -> bindgen::Builder {
        if let Ok(lib_dir) = env::var("NFC_NCI_LINUX_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", lib_dir);
        }
        // Tell cargo to invalidate the built crate whenever the wrapper changes
        println!("cargo:rerun-if-changed=wrapper.h");

        let bindings = bindgen::Builder::default().header("wrapper.h");

        if let Ok(include_dir) = env::var("NFC_NCI_LINUX_INCLUDE_DIR") {
            println!("cargo:include={}", include_dir);
            bindings.clang_arg(format!("-I/{}", include_dir))
        } else {
            bindings
        }
    }
}

#[cfg(feature = "vendored")]
mod lib {
    use bindgen;
    use std::env;
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Command;

    pub fn get_builder() -> bindgen::Builder {
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        let source_dir = Path::new("vendor/linux_libnfc-nci");
        if !source_dir.join(".git").exists() {
            Command::new("git")
                .args(&["submodule", "update", "--init"])
                .status()
                .expect("Git submodule update failed on linux_libnfc-nci.");
        }
        let target = env::var("TARGET").unwrap();

        // Run bootstrap
        Command::new("sh")
            .arg("./bootstrap")
            .current_dir(&source_dir)
            .status()
            .expect("Failed to run bootstrap");

        // Configure
        Command::new("sh")
            .args(&["./configure", &format!("--prefix={}", out_path.display())])
            .current_dir(&source_dir)
            .status()
            .expect("Failed to configure");

        // Make
        Command::new("make")
            .current_dir(&source_dir)
            .status()
            .expect("Failed to make");

        // Make install
        Command::new("make")
            .arg("install")
            .current_dir(&source_dir)
            .status()
            .expect("Failed to make install");

        let lib_dir = Path::new(&out_path).join("lib");
        // Tell cargo to link the library
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        bindgen::Builder::default()
            .header(
                source_dir
                    .join("src/include/linux_nfc_api.h")
                    .to_str()
                    .unwrap(),
            )
            .clang_arg(format!("--target={}", target))
    }
}

fn main() {
    println!("cargo:rustc-link-lib=dylib=nfc_nci_linux");

    let bindings = lib::get_builder()
        .default_macro_constant_type(bindgen::MacroTypeVariation::Signed)
        .derive_default(true)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
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
