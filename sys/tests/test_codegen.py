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

"""Tests of sys/codegen.py over small catalogs: each test states one rule the FFI
#Emitter writes follows, or one entry #CodegenError refuses, so a change to the
generator that breaks the ABI fails here before the drift job compares the
committed FFI. This repository has no other Python test suite; the nearest one is
MEOS-API's tests/test_stand_in_headers.py, whose tests over small synthetic inputs
these mirror.

Run:  python3 sys/tests/test_codegen.py
"""
import importlib.util
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "codegen", Path(__file__).resolve().parents[1] / "codegen.py")
codegen = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(codegen)


def catalog(**sections):
    """A catalog holding the given sections and empty others, the input #Emitter reads."""
    base = {"functions": [], "structs": [], "enums": [], "macros": [], "callbacks": [],
            "typedefs": []}
    base.update(sections)
    return base


def slot(name, canonical, **extra):
    """A catalog slot, its declared and canonical spellings alike unless `extra` says."""
    return {"name": name, "cType": canonical, "canonical": canonical, **extra}


def function(name, params=(), ret="void", **extra):
    """A catalog function, as MEOS-API's extract_function writes one: each parameter a
    (name, canonical) pair or a #slot."""
    return {"name": name, "returnType": {"c": ret, "canonical": ret},
            "params": [p if isinstance(p, dict) else slot(*p) for p in params], **extra}


def generate(**sections):
    """The FFI #Emitter writes for a catalog of these sections."""
    return codegen.Emitter(catalog(**sections)).generate()


class TypedefTests(unittest.TestCase):
    def test_a_kept_name_is_its_definition(self):
        out = generate(typedefs=[{"name": "TimestampTz", "canonical": "int64_t"},
                                 {"name": "Datum", "canonical": "uintptr_t"}],
                       functions=[function("f", [("t", "TimestampTz")], "Datum")])
        self.assertIn("pub type TimestampTz = i64;", out)
        self.assertIn("pub type Datum = usize;", out)
        self.assertIn("pub fn f(t: TimestampTz) -> Datum;", out)

    def test_text_is_varlena(self):
        out = generate(typedefs=[{"name": "text", "canonical": "struct varlena"}],
                       structs=[{"name": "varlena", "fields": []}])
        self.assertIn("pub type text = varlena;", out)

    def test_stdint_types_are_fixed_width(self):
        out = generate(functions=[function("f", [("x", "int64_t")], "uint32_t")])
        self.assertIn("pub fn f(x: i64) -> u32;", out)

    def test_a_type_of_its_own_names_its_width(self):
        out = generate(functions=[function(
            "h3index_eq", [slot("a", "uint64_t", cType="H3Index", typedef="H3Index")],
            "bool")])
        self.assertIn("pub type H3Index = u64;", out)
        self.assertIn("pub fn h3index_eq(a: H3Index) -> bool;", out)

    def test_a_catalog_without_typedefs_stops_the_generator(self):
        idl = catalog()
        del idl["typedefs"]
        with self.assertRaises(codegen.CodegenError):
            codegen.Emitter(idl)


class VariadicTests(unittest.TestCase):
    def test_a_variadic_function_ends_in_an_ellipsis(self):
        out = generate(functions=[function(
            "meos_error", [("level", "int"), ("format", "const char *")],
            variadic=True)])
        self.assertIn("pub fn meos_error(level: ::std::os::raw::c_int, "
                      "format: *const ::std::os::raw::c_char, ...);", out)

    def test_a_fixed_arity_function_has_none(self):
        out = generate(functions=[function("g", [("x", "int")])])
        self.assertNotIn("...", out)


class LayoutTests(unittest.TestCase):
    STRUCT = {"name": "Span", "sizeBytes": 24, "alignBytes": 8, "fields": [
        {**slot("spantype", "uint8_t"), "offset_bits": 0},
        {**slot("lower", "uintptr_t"), "offset_bits": 64},
        {**slot("flag", "int"), "offset_bits": 131}]}

    def test_size_alignment_and_byte_offsets_are_asserted(self):
        out = generate(structs=[self.STRUCT])
        self.assertIn('["Size of Span"][::std::mem::size_of::<Span>() - 24usize];', out)
        self.assertIn('["Alignment of Span"][::std::mem::align_of::<Span>() - 8usize];', out)
        self.assertIn("offset_of!(Span, lower) - 8usize", out)

    def test_a_bit_field_offset_is_not_asserted(self):
        self.assertNotIn("offset_of!(Span, flag)", generate(structs=[self.STRUCT]))

    def test_a_declared_struct_gets_no_assertion(self):
        out = generate(structs=[{"name": "NumericData", "fields": []}])
        self.assertNotIn("Size of NumericData", out)

    def test_a_pointer_typedef_is_a_pointer_to_its_record(self):
        out = generate(structs=[{"name": "NumericData", "fields": []}],
                       functions=[function("numeric_abs", [("n", "struct NumericData *")],
                                           "struct NumericData *")])
        self.assertIn("pub fn numeric_abs(n: *mut NumericData) -> *mut NumericData;", out)


class CallbackTests(unittest.TestCase):
    def test_a_callback_is_a_typed_function_pointer(self):
        out = generate(
            typedefs=[{"name": "Datum", "canonical": "uintptr_t"}],
            callbacks=[{"name": "datum_func2",
                        "returnType": {"c": "Datum", "canonical": "Datum"},
                        "params": [slot("arg0", "Datum"), slot("arg1", "Datum")]}])
        self.assertIn('pub type datum_func2 = ::std::option::Option<unsafe extern "C" '
                      "fn(arg0: Datum, arg1: Datum) -> Datum>;", out)

    def test_a_callback_parameter_is_passed_as_its_pointer(self):
        out = generate(callbacks=[{"name": "interrupt_handler_fn",
                                   "returnType": {"c": "void", "canonical": "void"},
                                   "params": []}],
                       functions=[function("install", [("h", "interrupt_handler_fn")])])
        self.assertIn("pub fn install(h: interrupt_handler_fn);", out)
        self.assertNotIn("pub struct interrupt_handler_fn", out)

    def test_an_inline_function_pointer_is_typed(self):
        out = generate(functions=[function(
            "skiplist_make", [("comp_fn", "int (*)(void *, void *)")])])
        self.assertIn('pub fn skiplist_make(comp_fn: ::std::option::Option<unsafe extern '
                      '"C" fn(arg1: *mut ::std::os::raw::c_void, '
                      'arg2: *mut ::std::os::raw::c_void) -> ::std::os::raw::c_int>);', out)


class FailureTests(unittest.TestCase):
    def test_an_unknown_type_by_value_stops_the_generator(self):
        with self.assertRaises(codegen.CodegenError):
            generate(functions=[function("f", [("x", "Mystery")])])

    def test_an_unknown_name_by_pointer_stops_the_generator(self):
        with self.assertRaises(codegen.CodegenError):
            generate(functions=[function("f", [("x", "Mystery *")])])

    def test_a_record_only_struct_names_is_opaque(self):
        out = generate(functions=[function("f", [("x", "struct json_object *")])])
        self.assertIn("pub struct json_object {", out)
        self.assertIn("pub fn f(x: *mut json_object);", out)

    def test_a_va_list_parameter_is_a_pointer_to_its_tag(self):
        out = generate(functions=[function("vf", [("ap", "va_list")])])
        self.assertIn("pub fn vf(ap: *mut __va_list_tag);", out)
        self.assertIn("pub struct __va_list_tag {", out)


class DocTests(unittest.TestCase):
    def test_a_doc_is_emitted_escaped(self):
        out = generate(functions=[function("f", doc='Return the "first" \\ value')])
        self.assertIn('#[doc = " Return the \\"first\\" \\\\ value"]', out)


if __name__ == "__main__":
    unittest.main()
