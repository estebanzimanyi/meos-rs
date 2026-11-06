#!/usr/bin/env python3
# NO-EXISTING-TOOL: this is meos-sys's catalog-driven FFI generator; no prior
# generator exists (the FFI is otherwise produced by rust-bindgen).
# BINDING-HEADER-PARSE-OK: reads the MEOS-API catalog `meos-idl.json`, never a
# `.h` header — the `read_text` call loads that JSON.
"""Generate the ``meos-sys`` FFI from the MEOS-API catalog (``meos-idl.json``).

``meos-sys`` is a low-level, raw FFI to libmeos. Its bindings are a pure
projection of the MEOS-API catalog — the one place a MEOS header is ever parsed
(MEOS-API ``run.py``, libclang). This generator consumes the catalog's
``structs`` / ``functions`` / ``enums`` / ``macros`` arrays and emits a single
``src/generated.rs``:

  * ``pub type`` aliases for every name the catalog's ``typedefs`` defines
    (``Datum``, ``TimestampTz``, ``text`` …) and every type of its own a slot's
    ``typedef`` names (``H3Index``), each the Rust type of its definition;
  * ``#[repr(C)]`` structs from ``structs`` (offset-ordered fields), opaque
    forward declarations for the ones the public headers leave incomplete;
  * ``pub type <E> = c_uint`` + ``pub const <E>_<V>`` for each ``enum``
    (matching rust-bindgen's constified-enum naming, which the upper ``meos``
    crate consumes as ``interpType_DISCRETE`` etc.);
  * ``pub const`` for each public object-like integer ``#define`` (``WKB_NDR`` …);
  * one ``extern "C"`` block per function.

The catalog states the surface an installed MEOS declares, so every entry is
projected.

Run:  python3 codegen.py <meos-idl.json> <out.rs>
"""
import json
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# The C language's own types, as Rust spells them. Every other name a type
# spelling uses is one the catalog states (#Emitter): a structure, an enumeration,
# a callback, a `typedefs` entry (`TimestampTz`, `Datum`, `text`), or a slot's
# `typedef` (`H3Index`), so this table holds C, not MEOS.
# ---------------------------------------------------------------------------
C_SCHAR = "::std::os::raw::c_schar"
C_SHORT = "::std::os::raw::c_short"
C_INT = "::std::os::raw::c_int"
C_LONG = "::std::os::raw::c_long"
C_UCHAR = "::std::os::raw::c_uchar"
C_USHORT = "::std::os::raw::c_ushort"
C_UINT = "::std::os::raw::c_uint"
C_ULONG = "::std::os::raw::c_ulong"
C_CHAR = "::std::os::raw::c_char"
C_VOID = "::std::os::raw::c_void"

# The fixed-width and size types are Rust's fixed-width types, as rust-bindgen
# spells them: `long` is 64 bits on Linux and 32 on Windows, so `int64_t` is
# `i64`, never `c_long`.
C_TYPES = {
    "bool": "bool",
    "char": C_CHAR,
    "signed char": C_SCHAR,
    "int": C_INT,
    "short": C_SHORT,
    "long": C_LONG,
    "unsigned int": C_UINT,
    "unsigned char": C_UCHAR,
    "unsigned short": C_USHORT,
    "unsigned long": C_ULONG,
    "double": "f64",
    "float": "f32",
    "int8_t": "i8",
    "int16_t": "i16",
    "int32_t": "i32",
    "int64_t": "i64",
    "uint8_t": "u8",
    "uint16_t": "u16",
    "uint32_t": "u32",
    "uint64_t": "u64",
    "size_t": "usize",
    "uintptr_t": "usize",
    "intptr_t": "isize",
    "ptrdiff_t": "isize",
    # <stdarg.h>'s va_list, as the x86-64 System V ABI passes it and rust-bindgen
    # declares it: a pointer to the platform's `__va_list_tag`
    "va_list": "*mut __va_list_tag",
}

# Rust keywords + primitives that cannot be used bare as an identifier; a
# colliding field / parameter / function name is suffixed with `_` (matching
# rust-bindgen: `str` -> `str_`, `type` -> `type_`).
RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
    "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
    "super", "trait", "true", "type", "unsafe", "use", "where", "while",
    "async", "await", "dyn", "abstract", "become", "box", "do", "final",
    "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
    "try", "str",
}


class CodegenError(Exception):
    """A catalog entry the generator cannot declare faithfully; #main reports it and
    exits with an error, so the drift job fails instead of committing a wrong FFI."""


def rust_string(text: str) -> str:
    """A Rust string literal holding `text`, as rust-bindgen writes a `#[doc]`: the
    backslash and the double quote escaped. Sibling of #sanitize_ident, which makes a
    C name a Rust identifier."""
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def sanitize_ident(name: str, index: int) -> str:
    if not name:
        return f"arg{index + 1}"
    return f"{name}_" if name in RUST_KEYWORDS else name


class Emitter:
    def __init__(self, idl: dict):
        if "typedefs" not in idl:
            raise CodegenError(
                "the catalog states no `typedefs`: provision it with a MEOS-API that "
                "states every type a slot names")
        self.struct_names = {s["name"] for s in idl["structs"]}
        self.enum_names = {e["name"] for e in idl["enums"]}
        self.structs = idl["structs"]
        self.enums = idl["enums"]
        self.macros = idl.get("macros", [])
        self.functions = idl["functions"]
        self.callbacks = idl.get("callbacks", [])
        self.callback_names = {c["name"] for c in self.callbacks}
        # The definition of each name a slot keeps (`TimestampTz` is `int64_t`,
        # `text` is `struct varlena`), and of each type of its own a slot names over
        # a width (`H3Index` over `uint64_t`), collected by #slot_type.
        self.typedefs = {t["name"]: t["canonical"] for t in idl["typedefs"]}
        self.own: dict[str, str] = {}
        # Structures only `struct` names and the catalog does not lay out
        # (`struct json_object`), reachable only through a pointer; collected while
        # mapping, emitted as zero-sized forward declarations.
        self.opaque: set[str] = set()

    # -- C type -> Rust FFI type -------------------------------------------
    def base_rust(self, name: str, by_value: bool = False, tagged: bool = False) -> str:
        """The Rust type of a C base type name, for #rust_type, which strips the
        pointers and arrays around it and says whether the name is used by value and
        whether `struct` names it. A name the catalog states keeps its name; a name
        only `struct` names is an opaque record, declared once and reachable only
        through a pointer. Any other name, and an opaque record used by value, stops
        the generator (#CodegenError) rather than declare a stand-in."""
        name = name.strip()
        if name == "void":
            return C_VOID
        if name in C_TYPES:
            if name == "va_list":
                self.opaque.add("__va_list_tag")
            return C_TYPES[name]
        if name in self.typedefs or name in self.own or name in self.callback_names \
                or name in self.struct_names or name in self.enum_names:
            return name
        if not tagged or not re.match(r"^[A-Za-z_]\w*$", name):
            raise CodegenError(f"the catalog states no type `{name}`")
        if by_value:
            raise CodegenError(
                f"`struct {name}` is used by value, and the catalog states no layout")
        self.opaque.add(name)
        return name

    def slot_type(self, slot: dict, parameter: bool = True) -> str:
        """The Rust type of a catalog slot, read by #rust_type from its `canonical`, the
        type as MEOS declares it. A slot naming a type of its own over a width
        (`typedef`: `H3Index` over `uint64_t`) is declared with that name, which
        #emit_aliases defines as the width. A parameter of array type is a pointer to
        its element, as C adjusts it (C11 6.7.6.3), where a struct field keeps its
        array (#emit_structs)."""
        spelling = slot["canonical"].strip()
        own = slot.get("typedef")
        if own:
            base = re.sub(r"\b(?:const|struct)\b|\*|\[\d*\]", " ", spelling).strip()
            self.own[own] = self.rust_type(base)
            spelling = re.sub(rf"\b{re.escape(base)}\b", own, spelling)
        if parameter:
            m = re.match(r"^(.*?)\s*\[\d*\]$", spelling)
            spelling = f"{m.group(1)} *" if m else spelling
        return self.rust_type(spelling)

    def function_pointer(self, ret: str, params: str) -> str:
        """An inline function pointer `T (*)(A, B)` as rust-bindgen declares one, a
        typed `Option<unsafe extern "C" fn(arg1: A, arg2: B) -> T>`, each part mapped
        by #rust_type as #emit_callbacks maps a named callback's."""
        parts = [p.strip() for p in params.split(",") if p.strip()]
        if parts == ["void"]:
            parts = []
        args = ", ".join(f"arg{i + 1}: {self.rust_type(p)}" for i, p in enumerate(parts))
        ret = ret.strip()
        arrow = "" if ret == "void" else f" -> {self.rust_type(ret)}"
        return f'::std::option::Option<unsafe extern "C" fn({args}){arrow}>'

    def rust_type(self, ctype: str) -> str:
        s = ctype.strip()

        # Array: `T[N]` (fixed) or `T[]` (flexible member -> zero-length array).
        m = re.match(r"^(.*?)\s*\[(\d*)\]$", s)
        if m:
            inner = self.rust_type(m.group(1).strip())
            n = m.group(2)
            return f"[{inner}; 0]" if n == "" else f"[{inner}; {n}usize]"

        # Function pointer: `T (*)(…)`.
        m = re.match(r"^(.*?)\(\s*\*\s*\)\s*\((.*)\)$", s)
        if m:
            return self.function_pointer(m.group(1), m.group(2))
        if "(" in s:
            raise CodegenError(f"no Rust type for the C type `{s}`")

        stars = s.count("*")
        without_stars = s.replace("*", "")
        tagged = bool(re.search(r"\b(?:struct|union)\b", without_stars))
        base_name = re.sub(r"\b(const|struct|union|enum)\b", " ", without_stars)
        base_name = " ".join(base_name.split())

        if stars == 0:
            return self.base_rust(base_name, by_value=True, tagged=tagged)

        pointee_const = bool(re.match(r"^\s*const\b", without_stars.strip()))
        inner = self.base_rust(base_name, tagged=tagged)
        rt = f"*{'const' if pointee_const else 'mut'} {inner}"
        for _ in range(stars - 1):  # outer pointers are non-const here
            rt = f"*mut {rt}"
        return rt

    # -- emitters -----------------------------------------------------------
    def emit_aliases(self) -> str:
        """`pub type` for each name the catalog's `typedefs` defines and each type of
        its own a slot names (#slot_type), as rust-bindgen declares a typedef:
        `TimestampTz` is `i64`, `text` is `varlena`, `H3Index` is `u64`."""
        aliases = {n: self.rust_type(c) for n, c in self.typedefs.items()}
        aliases.update(self.own)
        return "\n".join(f"pub type {n} = {t};" for n, t in sorted(aliases.items()))

    def emit_callbacks(self) -> str:
        """One typed `Option<unsafe extern "C" fn(…)>` per catalog callback, as
        rust-bindgen writes a function-pointer typedef, so a caller passes a Rust
        `fn` (`Some(handler)`). Its parameters and result map as #emit_functions maps
        a function's."""
        out = []
        for cb in self.callbacks:
            params = [f"{sanitize_ident(p.get('name', ''), i)}: {self.slot_type(p)}"
                      for i, p in enumerate(cb["params"])]
            if cb.get("variadic"):
                params.append("...")
            ret = cb["returnType"]
            arrow = "" if ret["canonical"].strip() == "void" \
                else f" -> {self.slot_type(ret, parameter=False)}"
            out.append(f"pub type {cb['name']} = ::std::option::Option<"
                       f"unsafe extern \"C\" fn({', '.join(params)}){arrow}>;")
        return "\n".join(out)

    def emit_opaque(self) -> str:
        # Emitted after mapping so `self.opaque` is fully populated.
        lines = []
        for name in sorted(self.opaque):
            lines.append("#[repr(C)]")
            lines.append("#[derive(Debug, Copy, Clone)]")
            lines.append(f"pub struct {name} {{")
            lines.append("    _unused: [u8; 0],")
            lines.append("}")
        return "\n".join(lines)

    def emit_enums(self) -> str:
        out = []
        for e in self.enums:
            name = e["name"]
            values = e["values"]
            repr_ty = C_INT if any(v["value"] < 0 for v in values) else C_UINT
            for v in values:
                out.append(f"pub const {name}_{v['name']}: {name} = {v['value']};")
            out.append(f"pub type {name} = {repr_ty};")
        return "\n".join(out)

    def emit_macros(self) -> str:
        out = []
        for mac in sorted(self.macros, key=lambda x: x["name"]):
            value = mac["value"]
            if isinstance(value, bool):
                out.append(f"pub const {mac['name']}: bool = {str(value).lower()};")
                continue
            ty = "i32" if value < 0 else "u32"
            out.append(f"pub const {mac['name']}: {ty} = {value};")
        return "\n".join(out)

    def emit_structs(self) -> str:
        out = []
        for s in self.structs:
            out.append("#[repr(C)]")
            out.append("#[derive(Debug, Copy, Clone)]")
            out.append(f"pub struct {s['name']} {{")
            if not s["fields"]:
                out.append("    _unused: [u8; 0],")
            else:
                for i, f in enumerate(s["fields"]):
                    fname = sanitize_ident(f["name"], i)
                    out.append(f"    pub {fname}: {self.slot_type(f, parameter=False)},")
            out.append("}")
            out.extend(self.layout_assertions(s))
        return "\n".join(out)

    def layout_assertions(self, s: dict) -> list[str]:
        """Compile-time checks that the Rust struct has the size, alignment and field
        offsets the catalog states (`sizeBytes`, `alignBytes`, `offset_bits`), in
        rust-bindgen's form. A field at no whole byte (a bit-field) has no Rust offset to
        check. A struct the catalog only declares states no size and gets none. Sibling
        of #emit_structs, which lays the fields out."""
        if not s["fields"] or "sizeBytes" not in s:
            return []
        name = s["name"]
        out = ["#[allow(clippy::unnecessary_operation, clippy::identity_op)]",
               "const _: () = {",
               f'    ["Size of {name}"][::std::mem::size_of::<{name}>() - '
               f'{s["sizeBytes"]}usize];']
        if "alignBytes" in s:
            out.append(f'    ["Alignment of {name}"][::std::mem::align_of::<{name}>() - '
                       f'{s["alignBytes"]}usize];')
        for i, f in enumerate(s["fields"]):
            bits = f.get("offset_bits")
            if not isinstance(bits, int) or bits < 0 or bits % 8:
                continue
            fname = sanitize_ident(f["name"], i)
            out.append(f'    ["Offset of field: {name}::{fname}"]'
                       f"[::std::mem::offset_of!({name}, {fname}) - {bits // 8}usize];")
        out.append("};")
        return out

    def emit_functions(self) -> str:
        out = []
        for fn in self.functions:
            params = []
            for i, p in enumerate(fn["params"]):
                pname = sanitize_ident(p.get("name", ""), i)
                params.append(f"{pname}: {self.slot_type(p)}")
            ret = fn["returnType"]
            arrow = "" if ret["canonical"].strip() == "void" \
                else f" -> {self.slot_type(ret, parameter=False)}"
            fname = fn["name"]
            if fname in RUST_KEYWORDS:
                fname = f"r#{fname}"
            if fn.get("variadic"):
                params.append("...")
            out.append('extern "C" {')
            if fn.get("doc"):
                out.append(f"    #[doc = {rust_string(' ' + fn['doc'])}]")
            out.append(f"    pub fn {fname}({', '.join(params)}){arrow};")
            out.append("}")
        return "\n".join(out)

    def generate(self) -> str:
        # Map structs / functions first so `self.opaque` is complete before the
        # opaque block is emitted.
        structs = self.emit_structs()
        functions = self.emit_functions()
        callbacks = self.emit_callbacks()
        enums = self.emit_enums()
        aliases = self.emit_aliases()  # after every slot, so #slot_type filled `own`
        macros = self.emit_macros()
        opaque = self.emit_opaque()

        return "\n".join([
            "// @generated by codegen.py from the MEOS-API catalog "
            "(meos-idl.json). Do not edit by hand.",
            "//",
            "// The catalog is the single source of truth for every MEOS "
            "binding; this",
            "// FFI is a pure projection of it. Regenerate with "
            "`python3 codegen.py`.",
            "",
            "// --- base-type vocabulary ---",
            aliases,
            "",
            "// --- callbacks ---",
            callbacks,
            "",
            "// --- external opaque types ---",
            opaque,
            "",
            "// --- enums ---",
            enums,
            "",
            "// --- #define constants ---",
            macros,
            "",
            "// --- structs ---",
            structs,
            "",
            "// --- functions ---",
            functions,
            "",
        ])


def main() -> None:
    """Write the FFI of the catalog with #Emitter."""
    if len(sys.argv) != 3:
        sys.exit("usage: codegen.py <meos-idl.json> <out.rs>")
    idl = json.loads(Path(sys.argv[1]).read_text())
    try:
        out = Emitter(idl).generate()
    except CodegenError as err:
        sys.exit(f"codegen.py: {err}")
    Path(sys.argv[2]).write_text(out)
    print(f"wrote {sys.argv[2]}")


if __name__ == "__main__":
    main()
