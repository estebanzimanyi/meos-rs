
const MINIMUM_MEOS_VERSION: &str = "1.4";

/// Detect MEOS config parameters using pkg-config (not available for all MEOS
/// versions)
fn detect_meos_via_pkg_config() -> Result<pkg_config::Library, pkg_config::Error> {
    use pkg_config::Config;

    Config::new()
        .atleast_version(MINIMUM_MEOS_VERSION)
        .probe("meos")
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // If `bundled` is on, use the git submodule to build MEOS from scratch
    let include_path = if cfg!(feature = "bundled") {
        // Defined in `meos-src` build.rs file
        let meos_path = std::env::var("DEP_MEOSSRC_SEARCH").unwrap();

        // Tell cargo to tell rustc to link the system meos shared library.
        println!("cargo:rustc-link-search=native={meos_path}/lib");
        println!("cargo:rustc-link-lib=static=meos");

        println!("cargo:rustc-link-lib=static=json-c");
        println!("cargo:rustc-link-lib=static=gsl");
        println!("cargo:rustc-link-lib=static=gslcblas");

        // Expose the path to spatial_ref_sys.csv for use in meos_initialize
        let csv_path = std::path::Path::new(&meos_path).join("share/spatial_ref_sys.csv");
        println!("cargo:SPATIAL_REF_SYS_CSV={}", csv_path.display());

        format!("{meos_path}/include")
    } else {
        let pk = detect_meos_via_pkg_config();

        match pk {
            Ok(meos) => meos.include_paths[0].clone().display().to_string(),
            Err(pkg_config_err) => {
                if matches!(pkg_config_err, pkg_config::Error::Command { ref cause, .. } if cause.kind() == std::io::ErrorKind::NotFound)
                {
                    panic!("Could not find `pkg-config` in your path. Please install it before running meos-sys-bind.");
                }
                panic!("Could not detect MEOS {MINIMUM_MEOS_VERSION} using pkg-config: {pkg_config_err}");
            }
        }
    };

    #[cfg(feature = "bindgen")]
    generate_bindings(include_path.into()).unwrap();

    #[cfg(not(feature = "bindgen"))]
    let _ = include_path;
}

/// Generate the FFI with bindgen over the installed MEOS headers, as
/// #generate_bindings always has, keeping only the names of the MEOS-API catalog:
/// `allowlist.txt`, written with `wrapper.h` by `allowlist.py`, lists one
/// `function`, `type` or `var` per catalog entry and one `define` per family the
/// headers guard. Each name is a C identifier anchored at both ends, so bindgen
/// emits that symbol exactly and the C library declarations the headers include
/// stay out.
#[cfg(feature = "bindgen")]
fn generate_bindings(include_path: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=allowlist.txt");
    let allowlist = std::fs::read_to_string("allowlist.txt")?;

    let mut builder = bindgen::Builder::default()
        .clang_arg(format!("-I{}", include_path.to_string_lossy()))
        .header("wrapper.h");
    for line in allowlist.lines() {
        let Some((kind, name)) = line.split_once(' ') else {
            continue;
        };
        builder = match kind {
            "define" => builder.clang_arg(format!("-D{name}=1")),
            "function" => builder.allowlist_function(format!("^{name}$")),
            "type" => builder.allowlist_type(format!("^{name}$")),
            "var" => builder.allowlist_var(format!("^{name}$")),
            _ => builder,
        };
    }
    let bindings = builder.generate().expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_path.join("bindings.rs"))?;

    Ok(())
}
