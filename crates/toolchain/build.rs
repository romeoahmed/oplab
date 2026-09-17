//! Build the CXX bridge using the selected LLVM distribution and Cargo target.

#[path = "build/cpp.rs"]
mod cpp;
#[path = "build/sdk.rs"]
mod sdk;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let toolchain = sdk::Toolchain::discover()?;
    let mut build = cxx_build::bridges(["src/assembly/ffi.rs", "src/decode/ffi.rs"]);
    build
        .file("native/assembly.cpp")
        .file("native/link.cpp")
        .file("native/target.cpp")
        .file("native/decode.cpp")
        .file("native/x86.cpp")
        .file("native/aarch64.cpp")
        .std("c++23")
        .warnings_into_errors(true);
    cpp::configure(&mut build, &toolchain.includes)?;
    cpp::write_compilation_database(&build, &toolchain)?;
    build.try_compile("oplab-toolchain")?;
    toolchain.emit_link_metadata();
    for source in ["src/assembly/ffi.rs", "src/decode/ffi.rs", "native"] {
        println!("cargo::rerun-if-changed={source}");
    }
    Ok(())
}
