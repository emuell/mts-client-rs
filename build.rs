//! Builds the vendored ODDSound MTS-ESP C++ client impl.

use std::path::Path;

// -------------------------------------------------------------------------------------------------

fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set by cargo");
    let source_dir = Path::new(&manifest_dir).join("vendor/MTS-ESP/Client");

    let source = source_dir.join("libMTSClient.cpp");
    let header = source_dir.join("libMTSClient.h");
    if !source.exists() {
        panic!(
            "Missing {}.\n\
             The MTS-ESP client sources are a git submodule. Fetch them with:\n\
             \n    git submodule update --init --recursive\n",
            source.display()
        );
    }

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", header.display());

    let mut build = cc::Build::new();
    build.cpp(true).file(&source).include(&source_dir);

    if build.get_compiler().is_like_msvc() {
        build.flag("/EHsc");
    } else {
        // The client pulls in no C++ standard library headers and uses neither exceptions nor RTTI
        build
            .flag_if_supported("-std=c++11")
            .flag_if_supported("-fno-exceptions")
            .flag_if_supported("-fno-rtti");
    }

    build.compile("mtsclient");

    // dlopen/dlsym is folded into libc on glibc >= 2.34, but older glibc may still need the link.
    // macOS gets them from libSystem. Windows needs no link flag either.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=dylib=dl");
    }
}
