#!/usr/bin/env bash
# This MobilityDB code is provided under The PostgreSQL License.
# Copyright (c) 2016-2026, Université libre de Bruxelles and MobilityDB
# contributors
# Permission to use, copy, modify, and distribute this software and its
# documentation for any purpose, without fee, and without a written
# agreement is hereby granted, provided that the above copyright notice and
# this paragraph and the following two paragraphs appear in all copies.
# IN NO EVENT SHALL UNIVERSITE LIBRE DE BRUXELLES BE LIABLE TO ANY PARTY FOR
# DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, INCLUDING
# LOST PROFITS, ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS DOCUMENTATION,
# EVEN IF UNIVERSITE LIBRE DE BRUXELLES HAS BEEN ADVISED OF THE POSSIBILITY
# OF SUCH DAMAGE.
# UNIVERSITE LIBRE DE BRUXELLES SPECIFICALLY DISCLAIMS ANY WARRANTIES,
# INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY
# AND FITNESS FOR A PARTICULAR PURPOSE. THE SOFTWARE PROVIDED HEREUNDER IS ON
# AN "AS IS" BASIS, AND UNIVERSITE LIBRE DE BRUXELLES HAS NO OBLIGATIONS TO
# PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.

# regenerate.sh — rewrite sys/prebuilt-bindings/meos.rs with the pinned bindgen over the MEOS
# headers of one libmeos install, keeping the names of the MEOS-API catalog:
#
#   sys/regenerate.sh <meos-idl.json> <libmeos-prefix>
#
# allowlist.py writes sys/wrapper.h and sys/allowlist.txt from the catalog; the crate's own
# `bindgen` feature then runs bindgen over them, as a build against the installed headers does,
# catalog_docs.py gives each function the doc the catalog states for it, and the result becomes
# the committed FFI every other build includes.
set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <meos-idl.json> <libmeos-prefix>" >&2
  exit 2
fi
CATALOG="$1"
PREFIX="$2"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="$REPO/target/regenerate"

python3 "$REPO/sys/allowlist.py" "$CATALOG" "$PREFIX/include" "$REPO/sys"
rm -rf "$TARGET"
PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig" \
  cargo build --manifest-path "$REPO/sys/Cargo.toml" --features bindgen --target-dir "$TARGET"
mapfile -t OUT < <(find "$TARGET" -path '*/meos-sys-*/out/bindings.rs')
if [ "${#OUT[@]}" -ne 1 ]; then
  echo "expected one bindings.rs under $TARGET, found ${#OUT[@]}" >&2
  exit 1
fi
mkdir -p "$REPO/sys/prebuilt-bindings"
cp "${OUT[0]}" "$REPO/sys/prebuilt-bindings/meos.rs"
python3 "$REPO/sys/catalog_docs.py" "$CATALOG" "$REPO/sys/prebuilt-bindings/meos.rs"
echo "wrote sys/prebuilt-bindings/meos.rs"
