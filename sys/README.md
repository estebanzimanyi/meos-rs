# meos-sys

Low level [MEOS](https://libmeos.org/) C API bindings for MEOS.

It provides the C interface as is; the [meos](https://github.com/MobilityDB/meos-rs)
crate is its Rust-friendly layer.

You can also find it on [crates.io](https://crates.io/crates/meos).

MEOS 1.4 is supported.

## Usage

Select a feature depending on your setup:

| Feature    | Description |
|------------|-------------|
| (none)     | Use the prebuilt bindings for MEOS 1.4. Requires MEOS 1.4 installed on your system. |
| `bundled`  | Build MEOS 1.4 from source. No system MEOS required. Implies `bindgen`. |
| `bindgen`  | Generate the bindings at build time from the MEOS headers. |

```toml
# Cargo.toml

# Use system-installed MEOS 1.4 with prebuilt bindings
meos-sys = { version = "0.2.0" }

# Build MEOS from source (no system MEOS needed)
meos-sys = { version = "0.2.0", features = ["bundled"] }

# Generate bindings from your system MEOS headers at build time
meos-sys = { version = "0.2.0", features = ["bindgen"] }
```

## Build

### System-installed MEOS

`pkg-config` detects MEOS 1.4:

```bash
# Linux
PKG_CONFIG_PATH=<path>/lib/pkgconfig LD_LIBRARY_PATH=<path>/lib cargo build
```

### Bundled (`bundled`)

Builds MEOS 1.4 with every family, as MobilityDB's CI builds it (`-DALL=ON`),
from the bundled source as a static library, with JSON-C and H3 from their
submodules and PROJ through `proj-sys`. No system MEOS installation required.

The RASTER family links GDAL, which comes from the system and must be built
without GEOS, since MEOS 1.4 links no GEOS; a GDAL other than the system's is
named by `GDAL_ROOT`, as CMake's FindGDAL reads it. MEOS-API's `provision-meos`
action builds one with its `build-gdal.sh`.

The following system packages are required to compile:

- `cmake` — the build system compiling MEOS and its dependencies
- `clang` / `libclang-dev` — required by `bindgen` to parse the MEOS C headers and generate Rust bindings
- `pkg-config` — locates system libraries during the build
- `sqlite3` / `libsqlite3-dev` — required by PROJ, which uses SQLite to store its coordinate reference system database
- `libproj-dev`, `libxml2-dev`, `zlib1g-dev` — libraries MEOS and GDAL link

```bash
# Debian/Ubuntu
apt-get install cmake clang libclang-dev pkg-config sqlite3 libsqlite3-dev libproj-dev libxml2-dev zlib1g-dev
```

Additionally, the git submodules (MobilityDB, JSON-C, H3) must be initialized before building, as they are bundled as git submodules rather than downloaded at build time:

```bash
git submodule update --init --recursive
```

> **Note:** If you have a system `libmeos.so` installed (e.g. at
> `/usr/local/lib/libmeos.so`), remove or unload it before using `bundled` to
> avoid conflicts.

Compilation will take longer due to building all dependencies from source.

### Bindgen (`bindgen`)

Generates the bindings at build time with bindgen over the MEOS headers instead
of using the prebuilt ones, keeping only the names `allowlist.txt` lists: one per
function, type and constant of the [MEOS-API](https://github.com/MobilityDB/MEOS-API)
catalog `meos-idl.json`. `regenerate.sh` rewrites the prebuilt bindings,
`allowlist.txt` and `wrapper.h` from a catalog and an installed MEOS, and
`check_drift.py` checks that every bound function is a catalog function of the
same arity.
