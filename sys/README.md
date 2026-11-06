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
| (none)     | Link against a system-installed MEOS 1.4. |
| `bundled`  | Build MEOS 1.4 from source. No system MEOS required. |

The FFI itself is the same for both: `src/generated.rs`, targeting the MEOS 1.4
API. `pkg-config` locates a system MEOS; `bundled` builds MEOS and its
dependencies from the bundled source instead.

```toml
# Cargo.toml

# Use system-installed MEOS
meos-sys = { version = "0.2.0" }

# Build MEOS from source (no system MEOS needed)
meos-sys = { version = "0.2.0", features = ["bundled"] }
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
- `pkg-config` — locates system libraries during the build
- `sqlite3` / `libsqlite3-dev` — required by PROJ, which uses SQLite to store its coordinate reference system database
- `libproj-dev`, `libxml2-dev`, `zlib1g-dev` — libraries MEOS and GDAL link

```bash
# Debian/Ubuntu
apt-get install cmake pkg-config sqlite3 libsqlite3-dev libproj-dev libxml2-dev zlib1g-dev
```

Additionally, the git submodules (MobilityDB, JSON-C, H3) must be initialized before building, as they are bundled as git submodules rather than downloaded at build time:

```bash
git submodule update --init --recursive
```

> **Note:** If you have a system `libmeos.so` installed (e.g. at
> `/usr/local/lib/libmeos.so`), remove or unload it before using `bundled` to
> avoid conflicts.

Compilation will take longer due to building all dependencies from source.

## Generated bindings

`src/generated.rs` is a projection of the [MEOS-API](https://github.com/MobilityDB/MEOS-API)
catalog `meos-idl.json` — the single source of truth from which every MEOS
binding is derived, and the one place a MEOS header is ever parsed. `codegen.py`
emits from that catalog the `#[repr(C)]` structs with compile-time checks of
their size, alignment and field offsets, the `extern "C"` functions with their
documentation and the trailing `...` of a variadic one, the callbacks as typed
function pointers, the enums, and the `#define` constants. It stops on a type it
cannot declare faithfully rather than writing a stand-in:

```bash
python3 codegen.py <meos-idl.json> src/generated.rs
python3 tests/test_codegen.py
```
