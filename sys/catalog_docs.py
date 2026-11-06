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

"""Give each function bindgen declares the doc the MEOS-API catalog states for it.

MEOS documents a function where it defines it, in the .c file, so bindgen, which reads
the headers, sees a doc only where a header carries one. The catalog reads the
definitions; this writes its `doc` of each function as the `#[doc]` of the function's
declaration in the bindings bindgen wrote, in bindgen's own form, and drops the header's
where the catalog states none, so every function doc of the FFI comes from the catalog,
as allowlist.py takes every name from it.

usage: catalog_docs.py <meos-idl.json> <bindings.rs>
"""

import json
import re
import sys
from pathlib import Path

FN = re.compile(r"^    pub fn (\w+)\(")
DOC = re.compile(r"^    #\[doc = ")


def rust_string(text: str) -> str:
    """A Rust string literal holding `text`, as rust-bindgen writes a `#[doc]`: the
    backslash and the double quote escaped. The same literal as #rust_string of the
    catalog generator (sys/codegen.py of the catalog-generated FFI), so a doc reads
    alike in both."""
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main() -> None:
    """Rewrite the bindings in place, as #main of allowlist.py writes the bindgen
    inputs from the same catalog."""
    if len(sys.argv) != 3:
        sys.exit(__doc__.strip().splitlines()[-1])
    docs = {f["name"]: f["doc"] for f in json.loads(Path(sys.argv[1]).read_text())["functions"]
            if f.get("doc")}
    path = Path(sys.argv[2])
    out, written = [], 0
    for line in path.read_text().splitlines(keepends=True):
        m = FN.match(line)
        if m:
            # The function's own doc lines sit right above its declaration.
            while out and DOC.match(out[-1]):
                out.pop()
            if m[1] in docs:
                out.append(f"    #[doc = {rust_string(' ' + docs[m[1]])}]\n")
                written += 1
        out.append(line)
    path.write_text("".join(out))
    print(f"{written} function docs from the catalog")


if __name__ == "__main__":
    main()
