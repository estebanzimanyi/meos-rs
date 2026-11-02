//! Build MEOS from the MobilityDB submodule as a static library, with every family, as
//! MobilityDB's CI builds it (-DALL=ON). json-c and libh3 are built here from their
//! submodules, libh3 as #main has always built json-c, and PROJ by proj-sys; GDAL, which
//! the RASTER family links, libxml2 and zlib come from the system: no crate exposes a GDAL
//! build to a dependent C build, and the GDAL a MEOS build links is built without GEOS.
//! A GDAL other than the system's is named, as CMake's FindGDAL reads it, by GDAL_ROOT in the
//! environment. MEOS 1.4 links neither GEOS nor GSL.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // The C sources this script builds: a submodule moved to another commit rebuilds them
    println!("cargo:rerun-if-changed=source");
    println!("cargo:rerun-if-changed=json-c");
    println!("cargo:rerun-if-changed=h3");

    // Build json-c
    let json_c_path = cmake::Config::new("json-c")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_TESTING", "OFF")
        .build();

    // Build libh3, position-independent as MEOS requires of a static archive
    let h3_path = cmake::Config::new("h3")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON")
        .define("BUILD_TESTING", "OFF")
        .define("BUILD_BENCHMARKS", "OFF")
        .define("BUILD_FUZZERS", "OFF")
        .define("BUILD_FILTERS", "OFF")
        .define("BUILD_GENERATORS", "OFF")
        .define("ENABLE_DOCS", "OFF")
        .define("ENABLE_FORMAT", "OFF")
        .define("ENABLE_LINTING", "OFF")
        .build();
    let h3_lib_dir = ["lib", "lib64"]
        .iter()
        .map(|d| h3_path.join(d))
        .find(|d| d.join("libh3.a").exists())
        .expect("libh3.a not found in the h3 build");

    let mut config = cmake::Config::new("source");
    config
        .define("MEOS", "ON")
        .define("ALL", "ON")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("BUILD_TESTING", "OFF")
        .define("H3_INCLUDE_DIR", h3_path.join("include").join("h3"))
        .define("H3_LIBRARY", h3_lib_dir.join("libh3.a"))
        .very_verbose(true);

    // Collect include paths from dependencies
    let mut prefix_paths = vec![
        json_c_path.display().to_string(),
        h3_path.display().to_string(),
    ];

    if let Ok(proj_root) = std::env::var("DEP_PROJ_ROOT") {
        let root_path = std::path::Path::new(&proj_root);
        let include_path = root_path.join("include");
        let lib_path = root_path.join("lib").join("libproj.a");

        config.define("PROJ_INCLUDE_DIRS", include_path);
        config.define("PROJ_LIBRARIES", lib_path);
        prefix_paths.push(proj_root);
    } else if let Ok(proj_include) = std::env::var("DEP_PROJ_INCLUDE") {
        config.define("PROJ_INCLUDE_DIRS", &proj_include);

        // Infer the root from the include directory
        let path = std::path::Path::new(&proj_include);
        if let Some(parent) = path.parent() {
            prefix_paths.push(parent.to_string_lossy().to_string());

            let lib_path = parent.join("lib").join("libproj.a");
            if lib_path.exists() {
                config.define("PROJ_LIBRARIES", lib_path.as_os_str());
            } else {
                let build_lib_path = parent.join("build").join("lib").join("libproj.a");
                if build_lib_path.exists() {
                    config.define("PROJ_LIBRARIES", build_lib_path.as_os_str());
                }
            }
        }
    }

    // CMAKE_PREFIX_PATH is how find_package locates the libraries built above
    config.define("CMAKE_PREFIX_PATH", prefix_paths.join(";"));

    let libmeos = config.build();

    println!("cargo:lib=meos");
    let search_path = libmeos.display().to_string();
    assert!(std::path::Path::new(&search_path).exists());
    println!("cargo:search={}", search_path);

    // Link the static libraries built here
    println!("cargo:rustc-link-search=native={}/lib", json_c_path.display());
    println!("cargo:rustc-link-search=native={}/lib64", json_c_path.display());
    println!("cargo:rustc-link-lib=static=json-c");
    println!("cargo:rustc-link-search=native={}", h3_lib_dir.display());
    println!("cargo:rustc-link-lib=static=h3");

    // and the system libraries MEOS links
    println!("cargo:rustc-link-lib=dylib=gdal");
    println!("cargo:rustc-link-lib=dylib=xml2");
    println!("cargo:rustc-link-lib=dylib=z");
}
