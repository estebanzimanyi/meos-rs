#!/usr/bin/env python3
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

"""Write the bindgen inputs of meos-sys from the MEOS-API catalog.

The catalog says WHAT the MEOS API is, bindgen says HOW it maps to Rust. From
meos-idl.json this writes:
  wrapper.h      one #include per MEOS header the catalog's entries come from,
                 resolved under the installed include directory;
  allowlist.txt  one `function`, `type` or `var` line per catalog function,
                 struct, enum and macro, and one `define` line per family the
                 headers guard, the names build.rs hands to bindgen.

usage: allowlist.py <meos-idl.json> <include-dir> <out-dir>
"""

import json
import sys
from pathlib import Path


def main() -> None:
    if len(sys.argv) != 4:
        sys.exit(__doc__.strip().splitlines()[-1])
    catalog = json.loads(Path(sys.argv[1]).read_text())
    include = Path(sys.argv[2])
    out = Path(sys.argv[3])

    entries = catalog["functions"] + catalog["structs"] + catalog["enums"] + catalog.get("macros", [])
    installed = {p.name: p.relative_to(include).as_posix() for p in include.rglob("*.h")}
    headers = sorted({installed[e["file"]] for e in entries if e.get("file") in installed},
                     key=lambda h: (h != "meos.h", h != "meos_geo.h", "/" in h, h))
    (out / "wrapper.h").write_text("".join(f"#include <{h}>\n" for h in headers))

    lines = [f"define {family}" for family in catalog.get("families", [])]
    lines += [f"function {f['name']}" for f in catalog["functions"]]
    lines += [f"type {s['name']}" for s in catalog["structs"] + catalog["enums"]]
    lines += [f"var {m['name']}" for m in catalog.get("macros", [])]
    (out / "allowlist.txt").write_text("\n".join(sorted(set(lines))) + "\n")
    print(f"{len(headers)} headers, {len(lines)} allowlist entries")


if __name__ == "__main__":
    main()
