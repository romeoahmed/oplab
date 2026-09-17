//! Generate only Oplab's SDK boundary; QEMU's internal headers stay in the SDK build.
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    bindgen::Builder::default()
        .header("native/qemu.h")
        .clang_arg("-std=gnu23")
        .allowlist_type("Oplab.*")
        .allowlist_var("OPLAB_.*")
        .prepend_enum_name(false)
        .layout_tests(false)
        .generate_comments(false)
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()?
        .write_to_file(output.join("qemu.rs"))?;
    Ok(())
}
