const MINIMUM_MEOS_VERSION: &str = "1.4";

/// Detect MEOS config parameters using pkg-config (not available for all MEOS
/// versions)
fn detect_meos_via_pkg_config() -> Result<pkg_config::Library, pkg_config::Error> {
    use pkg_config::Config;

    Config::new()
        .atleast_version(MINIMUM_MEOS_VERSION)
        .probe("meos")
}

/// Locate and link libmeos: the one meos-src builds under `bundled`, or the system one
/// #detect_meos_via_pkg_config finds. The FFI is generated from the MEOS-API catalog by
/// `codegen.py` and committed as `src/generated.rs`, so no header is read here.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // If `bundled` is on, use the git submodule to build MEOS from scratch
    if cfg!(feature = "bundled") {
        // Defined in `meos-src` build.rs file
        let meos_path = std::env::var("DEP_MEOSSRC_SEARCH").unwrap();

        // Tell cargo to tell rustc to link the system meos shared library.
        println!("cargo:rustc-link-search=native={meos_path}/lib");
        println!("cargo:rustc-link-lib=static=meos");

        // The libraries libmeos.a needs, after it: the static ones meos-src builds and the
        // system ones it finds
        println!("cargo:rustc-link-lib=static=json-c");
        println!("cargo:rustc-link-lib=static=h3");
        println!("cargo:rustc-link-lib=dylib=gdal");
        println!("cargo:rustc-link-lib=dylib=xml2");
        println!("cargo:rustc-link-lib=dylib=z");

        // Expose the path to spatial_ref_sys.csv for use in meos_initialize
        let csv_path = std::path::Path::new(&meos_path).join("share/spatial_ref_sys.csv");
        println!("cargo:SPATIAL_REF_SYS_CSV={}", csv_path.display());
    } else if let Err(pkg_config_err) = detect_meos_via_pkg_config() {
        if matches!(pkg_config_err, pkg_config::Error::Command { ref cause, .. } if cause.kind() == std::io::ErrorKind::NotFound)
        {
            panic!("Could not find `pkg-config` in your path. Please install it before running meos-sys-bind.");
        }
        panic!("Could not detect MEOS {MINIMUM_MEOS_VERSION} using pkg-config: {pkg_config_err}");
    }
}
