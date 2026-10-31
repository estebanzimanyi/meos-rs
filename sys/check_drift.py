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

"""Check the committed meos-sys bindings against the MEOS-API catalog.

Every function the bindings declare must be a catalog function with the same
number of parameters, a variadic one carrying `...` after them; a binding the
catalog lacks or whose arity differs fails the check. A catalog function the
bindings lack is reported: the headers bindgen reads do not declare it.

It reads the catalog as #main of allowlist.py does, the one other reader of the
catalog in this crate; meos-rs has no binding check of its own to mirror.

usage: check_drift.py <meos-idl.json> <bindings.rs>
"""

import json
import re
import sys
from pathlib import Path

EXTERN = re.compile(r'extern "C" \{')
FN = re.compile(r"pub fn (\w+)\s*\(")


def arity(params: str) -> tuple[int, bool]:
    """The number of named parameters of a bindgen declaration and whether it ends
    with `...`. A parameter's type holds commas only inside angle brackets or
    parentheses (`Option<unsafe extern "C" fn(a: i32, b: i32)>`)."""
    depth, count, current, variadic = 0, 0, "", False
    for c in params + ",":
        if c in "<(":
            depth += 1
        elif c == ")" or (c == ">" and not current.endswith("-")):
            # the `>` of a return arrow `->` closes nothing
            depth -= 1
        if c == "," and depth == 0:
            item = current.strip()
            if item == "...":
                variadic = True
            elif item:
                count += 1
            current = ""
        else:
            current += c
    return count, variadic


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__.strip().splitlines()[-1])
    catalog = {f["name"]: len(f["params"]) for f in json.loads(Path(sys.argv[1]).read_text())["functions"]}
    # The functions the `extern "C"` blocks declare, each block and parameter list
    # closed by depth, as #arity counts it: a lazy pattern stops at the first `)` of a
    # function-pointer parameter, and the methods bindgen writes in `impl` blocks for
    # its helper types (`as_ptr`) are not FFI.
    text = Path(sys.argv[2]).read_text()
    bound = {}
    for block in EXTERN.finditer(text):
        depth, end = 0, block.end() - 1
        for end in range(block.end() - 1, len(text)):
            depth += {"{": 1, "}": -1}.get(text[end], 0)
            if depth == 0:
                break
        body = text[block.end():end]
        for fn in FN.finditer(body):
            depth = 0
            for close in range(fn.end() - 1, len(body)):
                depth += {"(": 1, ")": -1}.get(body[close], 0)
                if depth == 0:
                    break
            bound[fn[1]] = arity(body[fn.end():close])

    drift = []
    for name, (count, _variadic) in sorted(bound.items()):
        if name not in catalog:
            drift.append(f"{name}: bound but not in the catalog")
        elif count != catalog[name]:
            drift.append(f"{name}: {count} parameters bound, {catalog[name]} in the catalog")
    missing = sorted(set(catalog) - set(bound))

    print(f"catalog {len(catalog)} functions, bound {len(bound)}, drift {len(drift)}, unbound {len(missing)}")
    for line in drift:
        print("DRIFT", line)
    for name in missing:
        print("UNBOUND", name)
    sys.exit(1 if drift else 0)


if __name__ == "__main__":
    main()
