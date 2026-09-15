//! Build the CXX bridge using the selected LLVM distribution and Cargo target.

#[path = "build/cpp.rs"]
mod cpp;
#[path = "build/llvm.rs"]
mod llvm;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let toolchain = llvm::Toolchain::discover()?;
    let mut build = cxx_build::bridge("src/assembly/ffi.rs");
    build
        .file("native/assembly.cpp")
        .include("native")
        .std("c++23")
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true);
    cpp::configure(&mut build, &toolchain.includes)?;
    cpp::write_compilation_database(&build, &toolchain)?;
    build.try_compile("oplab-native")?;
    toolchain.emit_link_metadata()?;
    for source in ["src/assembly/ffi.rs", "native", "build", "build.rs"] {
        println!("cargo::rerun-if-changed={source}");
    }
    Ok(())
}
