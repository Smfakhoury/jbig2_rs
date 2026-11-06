import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from collect_compiler_inventory import (
    PACKAGE_FILES, TOOL_FILES, bindings, commands, digest, native_library, validate_existing,
)
from compiler_inventory import (
    compiler_items, declaration_surfaces, mir_bodies, reconcile, source_root_exports,
)


PREAMBLE = "// WARNING: This output format is intended for human consumers only\n"


def mir_function(name, parameters=""):
    return f"fn {name}({parameters}) -> () {{\n    bb0: {{\n        return;\n    }}\n}}\n"


def span(line, column, end_column):
    return {"filename": "src/lib.rs", "begin": [line, column], "end": [line, end_column]}


def item(identity, name, location, inner, attrs=None, visibility="default"):
    if "function" in inner:
        inner = {"function": {
            "sig": {"inputs": [], "output": None, "is_c_variadic": False},
            "generics": {"params": [], "where_predicates": []},
            "header": {"is_const": False, "is_unsafe": False, "is_async": False, "abi": "Rust"},
            **inner["function"],
        }}
    return {
        "id": identity, "name": name, "span": location, "crate_id": 0,
        "inner": inner, "attrs": attrs or [], "visibility": visibility,
    }


class CompilerInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text(
            "#[derive(Clone)]\nstruct Token(u8);\n"
            "fn keep(value: u8) -> u8 { let visit = || value; visit() }\n"
        )
        self.document = {
            "format_version": 61, "root": 0, "includes_private": True,
            "crate_version": "0.2.0-dev",
            "index": {
                "0": item(0, "jbig2_decoder", span(1, 1, 2),
                          {"module": {"items": [1, 2, 3]}}),
                "1": item(1, "Token", span(2, 1, 18),
                          {"struct": {"kind": {"tuple": [5]}}}),
                "2": item(2, "keep", span(3, 1, 55),
                          {"function": {"has_body": True}}),
                "3": item(3, None, span(1, 10, 15), {"impl": {
                    "items": [4], "trait": {"path": "Clone"},
                    "for": {"resolved_path": {"id": 1}},
                }}, ["automatically_derived"]),
                "4": item(4, "clone", span(1, 10, 15),
                          {"function": {"has_body": True}}),
                "5": item(5, "0", span(2, 14, 16), {"struct_field": {"primitive": "u8"}}),
            },
            "paths": {
                "1": {"crate_id": 0, "path": ["jbig2_decoder", "Token"]},
                "2": {"crate_id": 0, "path": ["jbig2_decoder", "keep"]},
            },
        }
        self.keep = mir_function("keep")
        self.clone = mir_function("<impl at src/lib.rs:1:10: 1:15>::clone")
        self.constructor = mir_function("Token")
        self.closure = mir_function(
            "keep::{closure#0}", "_1: {closure@src/lib.rs:3:38: 3:46}",
        )
        self.mir = PREAMBLE + self.keep + self.clone + self.constructor + self.closure
        self.mir += "// MIR FOR CTFE\n" + self.keep

    def test_exact_native_source_derive_closure_constructor_and_ctfe_accounting(self):
        result = reconcile(self.root, self.document, self.mir)
        self.assertEqual(result["counts"], {
            "explicit_source_declarations": 1, "compiler_function_items": 2,
            "compiler_derived_methods": 1, "abstract_declarations": 0,
            "executable_mir_bodies": 4, "closures": 1, "adt_constructors": 1,
            "separate_ctfe_bodies": 1, "owned_callable_and_abstract_identities": 4,
            "compiler_marker_implementations": 0,
        })
        self.assertEqual(result["derived_traits"], {"Clone": 1})
        self.assertFalse(result["ready_for_external_validation"])
        self.assertFalse(result["complete_production_inventory_qualified"])
        self.assertFalse(result["semantic_contract_quality_qualified"])
        self.assertFalse(result["public_entry_reachability_qualified"])
        self.assertTrue(all("proof_qualification" in row for row in result["records"]))
        self.assertEqual(result["api_declaration_surfaces"]["categories"], {
            "internal_closure_body": 1, "non_root_function_or_method": 2,
            "non_root_or_private_adt_constructor": 1,
        })
        self.assertEqual(result["api_declaration_surfaces"]["classified_owned_identities"], 4)

    def test_rejects_missing_private_exports_or_wrong_pinned_format(self):
        for field, value in (("includes_private", False), ("format_version", 60)):
            with self.subTest(field=field):
                document = copy.deepcopy(self.document)
                document[field] = value
                with self.assertRaisesRegex(ValueError, "schema|private"):
                    reconcile(self.root, document, self.mir)

    def test_rejects_missing_or_invented_explicit_declarations(self):
        document = copy.deepcopy(self.document)
        document["index"]["2"]["name"] = "invented"
        with self.assertRaisesRegex(ValueError, "unmapped compiler-owned"):
            reconcile(self.root, document, self.mir)

    def test_rejects_a_rehashed_omission_of_the_actual_source_derive(self):
        document = copy.deepcopy(self.document)
        document["index"]["3"]["inner"]["impl"]["items"] = []
        del document["index"]["4"]
        with self.assertRaisesRegex(ValueError, "unqualified empty|omits or invents source-derived"):
            reconcile(self.root, document, self.mir.replace(self.clone, ""))

    def test_rejects_fake_derive_markers_and_comment_tokens(self):
        document = copy.deepcopy(self.document)
        document["index"]["3"]["attrs"] = []
        with self.assertRaisesRegex(ValueError, "not an audited derive"):
            reconcile(self.root, document, self.mir)
        (self.root / "src/lib.rs").write_text(
            "//       Clone\nstruct Token(u8);\n"
            "fn keep(value: u8) -> u8 { let visit = || value; visit() }\n"
        )
        with self.assertRaisesRegex(ValueError, "actual derive attribute"):
            reconcile(self.root, self.document, self.mir)

    def test_rejects_wrong_generated_method_source_or_target(self):
        for mutate in (
            lambda doc: doc["index"]["3"]["inner"]["impl"]["trait"].update(path="Debug"),
            lambda doc: doc["index"]["1"].update(name="DifferentType"),
        ):
            with self.subTest(mutation=mutate):
                document = copy.deepcopy(self.document)
                mutate(document)
                with self.assertRaisesRegex(ValueError, "generated method|datatype"):
                    reconcile(self.root, document, self.mir)

    def test_rejects_omitted_datatypes_even_with_no_new_explicit_function(self):
        (self.root / "src/lib.rs").write_text(
            (self.root / "src/lib.rs").read_text() + "struct Hidden;\n",
        )
        with self.assertRaisesRegex(ValueError, "datatype inventory"):
            reconcile(self.root, self.document, self.mir)

    def test_rejects_empty_truncated_duplicate_and_unknown_mir_bodies(self):
        for text, message in (
            ("", "format"),
            (self.mir.replace(self.clone, ""), "body inventory"),
            (self.mir + self.clone, "duplicate executable"),
            (self.mir + mir_function("invented"), "unmapped"),
            (self.mir + "fn truncated() -> () {\n", "truncated"),
        ):
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                reconcile(self.root, self.document, text)

    def test_ctfe_is_a_separate_stage_not_an_extra_native_function(self):
        with self.assertRaisesRegex(ValueError, "corresponding executable"):
            mir_bodies(PREAMBLE + "// MIR FOR CTFE\n" + self.keep)
        with self.assertRaisesRegex(ValueError, "corresponding executable"):
            mir_bodies(self.mir + "// MIR FOR CTFE\n" + self.keep)

    def test_closures_need_exact_parents_and_real_source_spans(self):
        for text, message in (
            (self.mir.replace("keep::{closure#0}", "missing::{closure#0}"), "exact parent"),
            (self.mir.replace("{closure@src/lib.rs:3:38: 3:46}", "()"), "source span"),
            (self.mir.replace("closure@src/lib.rs", "closure@src/absent.rs"), "absent source"),
        ):
            with self.subTest(message=message), self.assertRaisesRegex(ValueError, message):
                reconcile(self.root, self.document, text)

    def test_adt_constructor_cannot_be_omitted_from_executable_inventory(self):
        with self.assertRaisesRegex(ValueError, "constructor inventory"):
            reconcile(self.root, self.document, self.mir.replace(self.constructor, ""))
        document = copy.deepcopy(self.document)
        document["index"]["1"]["inner"]["struct"]["kind"] = "unit"
        with self.assertRaisesRegex(ValueError, "constructor inventory"):
            reconcile(self.root, document, self.mir.replace(self.constructor, ""))

    def test_public_reexports_are_not_claimed_as_call_graph_reachability(self):
        document = copy.deepcopy(self.document)
        source = self.root / "src/lib.rs"
        source.write_text(source.read_text() + "pub use Token as PublicToken;\n")
        document["index"]["6"] = item(6, None, span(3, 1, 2), {"use": {
            "source": "Token", "name": "PublicToken", "id": 1, "is_glob": False,
        }}, visibility="public")
        document["index"]["0"]["inner"]["module"]["items"].append(6)
        result = reconcile(self.root, document, self.mir)
        self.assertEqual(result["public_root_exports"][0]["name"], "PublicToken")
        self.assertFalse(result["public_entry_reachability_qualified"])
        document["index"]["6"]["inner"]["use"]["is_glob"] = True
        with self.assertRaisesRegex(ValueError, "glob"):
            reconcile(self.root, document, self.mir)

    def test_source_export_completeness_rejects_a_rehashed_compiler_omission(self):
        source = self.root / "src/lib.rs"
        source.write_text(source.read_text() + "pub use Token as PublicToken;\n")
        with self.assertRaisesRegex(ValueError, "root reexports differ"):
            reconcile(self.root, self.document, self.mir)

    def test_rehashed_public_function_visibility_cannot_reclassify_source(self):
        document = copy.deepcopy(self.document)
        source = self.root / "src/lib.rs"
        source.write_text(source.read_text() + "impl Token {\n    fn hide() {}\n}\n")
        document["index"]["7"] = item(7, None, span(4, 1, 2), {"impl": {
            "items": [8], "trait": None, "for": {"resolved_path": {"id": 1}},
        }})
        document["index"]["8"] = item(
            8, "hide", span(5, 5, 17), {"function": {"has_body": True}},
        )
        mir = self.mir + mir_function("<impl at src/lib.rs:4:1: 6:2>::hide")
        reconcile(self.root, document, mir)
        document["index"]["8"]["visibility"] = "public"
        with self.assertRaisesRegex(ValueError, "visibility differs"):
            reconcile(self.root, document, mir)

    def test_source_export_paths_groups_aliases_comments_and_native_cfg(self):
        (self.root / "src/lib.rs").write_text(
            "pub use outer::{inner::{Token as Renamed, /* ignored */ Other}, Third};\n"
            "#[cfg(verus_keep_ghost)] pub use ghost::Only;\n"
            "#[cfg(any(fuzzing, jbig2_validation))] pub mod runtime_access;\n"
            "pub(crate) use internal::Private;\n",
        )
        self.assertEqual(source_root_exports(self.root), [
            ("outer::Third", "Third"), ("outer::inner::Other", "Other"),
            ("outer::inner::Token", "Renamed"),
        ])

    def test_new_public_module_or_glob_requires_explicit_source_export_review(self):
        for source in ("pub mod public_module {}", "pub use inner::*;"):
            (self.root / "src/lib.rs").write_text(source)
            with self.subTest(source=source), self.assertRaisesRegex(
                ValueError, "export-domain|reexport path",
            ):
                source_root_exports(self.root)

    def export(self, document, target, name, identity=100):
        document["index"][str(identity)] = item(
            identity, None, span(3, 1, 2), {"use": {
                "source": "inner::" + name, "name": name, "id": target, "is_glob": False,
            }}, visibility="public",
        )
        document["index"]["0"]["inner"]["module"]["items"].append(identity)

    def classify(self, document, closures=()):
        functions, constructors, parents = compiler_items(document)
        records = [{
            "id": "function:" + str(function["id"]), "compiler_id": function["id"],
            "origin": "explicit_source_declaration", "mir_identity": "f" + str(function["id"]),
        } for function in functions]
        records.extend({
            "id": "constructor:" + str(constructor["id"]), "compiler_id": constructor["id"],
            "origin": "compiler_adt_constructor", "mir_identity": "c" + str(constructor["id"]),
        } for constructor in constructors)
        records.extend(closures)
        result = declaration_surfaces(document, parents, records)
        return {row["id"]: row["declaration_surface"] for row in records}, result

    def test_root_free_function_aliases_keep_exact_compiler_signature(self):
        document = copy.deepcopy(self.document)
        self.export(document, 2, "read")
        self.export(document, 2, "read_again", 101)
        signature = document["index"]["2"]["inner"]["function"]
        signature["generics"]["where_predicates"].append({"bound_predicate": {"fixture": True}})
        rows, result = self.classify(document)
        row = rows["function:2"]
        self.assertEqual(row["category"], "root_reexported_function")
        self.assertEqual(row["root_aliases"], ["read", "read_again"])
        self.assertEqual(row["compiler_signature"]["generics"], signature["generics"])
        self.assertFalse(result["call_graph_reachability_qualified"])
        self.assertFalse(result["constructible_argument_domains_qualified"])

    def test_inherent_method_visibility_is_not_type_or_trait_visibility(self):
        document = copy.deepcopy(self.document)
        self.export(document, 1, "Token")
        document["index"]["7"] = item(7, None, span(3, 1, 2), {"impl": {
            "items": [8, 9], "trait": None, "for": {"resolved_path": {"id": 1}},
        }})
        for identity, visibility in ((8, "public"), (9, "default")):
            document["index"][str(identity)] = item(
                identity, "method", span(3, 1, 2),
                {"function": {"has_body": True}}, visibility=visibility,
            )
        rows, _ = self.classify(document)
        self.assertEqual(rows["function:8"]["category"], "public_inherent_method_on_root_type")
        self.assertEqual(rows["function:9"]["category"], "non_root_function_or_method")
        self.assertEqual(rows["function:4"]["category"], "trait_implementation_on_root_type")
        self.assertEqual(rows["function:9"]["root_aliases"], ["Token"])

    def test_public_methods_on_private_types_are_not_root_declarations(self):
        document = copy.deepcopy(self.document)
        document["index"]["4"]["visibility"] = "public"
        rows, _ = self.classify(document)
        self.assertEqual(rows["function:4"]["category"], "non_root_function_or_method")

    def test_exported_trait_callback_and_generic_impl_have_separate_roles(self):
        document = copy.deepcopy(self.document)
        document["index"]["7"] = item(7, "Sink", span(3, 1, 2), {
            "trait": {"items": [8]},
        }, visibility="public")
        document["index"]["8"] = item(
            8, "page", span(3, 1, 2), {"function": {"has_body": False}},
        )
        document["index"]["9"] = item(9, None, span(3, 1, 2), {"impl": {
            "items": [10], "trait": {"path": "Sink", "id": 7}, "for": {"generic": "F"},
        }})
        document["index"]["10"] = item(
            10, "page", span(3, 1, 2), {"function": {"has_body": True}},
        )
        self.export(document, 7, "Sink")
        rows, _ = self.classify(document)
        self.assertEqual(rows["function:8"]["category"], "root_trait_method")
        self.assertEqual(rows["function:10"]["category"], "non_root_function_or_method")

    def test_tuple_struct_constructor_needs_public_fields_not_just_exported_type(self):
        document = copy.deepcopy(self.document)
        self.export(document, 1, "Token")
        rows, _ = self.classify(document)
        self.assertEqual(rows["constructor:1"]["category"], "non_root_or_private_adt_constructor")
        document["index"]["5"]["visibility"] = "public"
        rows, _ = self.classify(document)
        self.assertEqual(rows["constructor:1"]["category"], "root_type_adt_constructor")
        document["index"]["1"]["inner"]["struct"]["kind"]["tuple"] = [None]
        rows, _ = self.classify(document)
        self.assertEqual(rows["constructor:1"]["category"], "non_root_or_private_adt_constructor")

    def test_enum_constructor_uses_enum_export_not_variant_default_visibility(self):
        document = copy.deepcopy(self.document)
        document["index"]["7"] = item(
            7, "Problem", span(3, 1, 2), {"enum": {"variants": [8]}}, visibility="public",
        )
        document["index"]["8"] = item(
            8, "Nested", span(3, 1, 2), {"variant": {"kind": {"tuple": [5]}}},
        )
        document["paths"]["8"] = {
            "crate_id": 0, "path": ["jbig2_decoder", "Problem", "Nested"],
        }
        self.export(document, 7, "Problem")
        rows, _ = self.classify(document)
        self.assertEqual(rows["constructor:8"]["category"], "root_type_adt_constructor")
        self.assertEqual(rows["constructor:8"]["target_compiler_id"], 7)
        self.assertEqual(rows["constructor:8"]["root_aliases"], ["Problem"])
        document["index"]["7"]["inner"]["enum"]["variants"] = []
        with self.assertRaisesRegex(ValueError, "owning enum"):
            self.classify(document)

    def test_nested_closures_remain_internal_even_under_a_public_function(self):
        document = copy.deepcopy(self.document)
        self.export(document, 2, "read")
        closures = [
            {"id": "inner", "origin": "compiler_closure", "mir_identity": "f2::{closure#0}::{closure#0}",
             "parent_mir_identity": "f2::{closure#0}"},
            {"id": "outer", "origin": "compiler_closure", "mir_identity": "f2::{closure#0}",
             "parent_mir_identity": "f2"},
        ]
        rows, result = self.classify(document, closures)
        self.assertEqual(result["categories"]["internal_closure_body"], 2)
        for identity in ("inner", "outer"):
            self.assertEqual(rows[identity], {
                "category": "internal_closure_body", "root_aliases": [],
                "enclosing_declaration_category": "root_reexported_function",
            })

    def test_ambiguous_owner_or_unqualified_signature_is_rejected(self):
        document = copy.deepcopy(self.document)
        self.export(document, 2, "read")
        del document["index"]["2"]["inner"]["function"]["sig"]
        with self.assertRaisesRegex(ValueError, "complete compiler signature"):
            self.classify(document)
        document = copy.deepcopy(self.document)
        document["index"]["7"] = item(7, "AliasOwner", span(3, 1, 2), {
            "trait": {"items": [4]},
        })
        with self.assertRaisesRegex(ValueError, "ambiguous callable"):
            self.classify(document)

    def test_unqualified_new_root_export_kind_is_rejected(self):
        document = copy.deepcopy(self.document)
        document["index"]["7"] = item(7, "Alias", span(3, 1, 2), {
            "type_alias": {"type": {"primitive": "u8"}},
        })
        self.export(document, 7, "Alias")
        with self.assertRaisesRegex(ValueError, "root export kind"):
            self.classify(document)

    def test_out_of_scope_compiler_source_spans_are_rejected(self):
        document = copy.deepcopy(self.document)
        document["index"]["2"]["span"]["filename"] = "../outside.rs"
        with self.assertRaisesRegex(ValueError, "out-of-scope"):
            reconcile(self.root, document, self.mir)

    def test_library_receipt_needs_real_nonempty_isolated_native_codegen(self):
        (self.root / "target").mkdir()
        library = self.root / "target/libjbig2_decoder.rlib"
        library.write_bytes(b"actual-test-receipt")
        message = {
            "reason": "compiler-artifact",
            "manifest_path": str(self.root / "Cargo.toml"),
            "target": {"kind": ["lib"], "name": "jbig2_decoder",
                       "src_path": str(self.root / "src/lib.rs")},
            "profile": {"test": False}, "filenames": [str(library)],
        }
        self.assertEqual(native_library(json.dumps(message), self.root), library)
        self.assertEqual(native_library(json.dumps(message), self.root / "src/.."), library)
        library.write_bytes(b"")
        with self.assertRaisesRegex(ValueError, "empty"):
            native_library(json.dumps(message), self.root)
        library.write_bytes(b"actual-test-receipt")
        message["profile"]["test"] = True
        with self.assertRaisesRegex(ValueError, "production library"):
            native_library(json.dumps(message), self.root)

    def test_implicit_and_copy_markers_remain_visible_without_function_credit(self):
        source = self.root / "src/lib.rs"
        source.write_text(source.read_text().replace("derive(Clone)", "derive(Clone, Copy)"))
        document = copy.deepcopy(self.document)
        for identity, trait, location in (
            (6, "Copy", span(1, 17, 21)), (7, "TrivialClone", span(1, 10, 15)),
        ):
            document["index"][str(identity)] = item(
                identity, None, location, {"impl": {
                    "items": [], "trait": {"path": trait},
                    "for": {"resolved_path": {"id": 1}},
                }}, ["automatically_derived"],
            )
        result = reconcile(self.root, document, self.mir)
        self.assertEqual(result["counts"]["compiler_marker_implementations"], 2)
        self.assertEqual(result["counts"]["executable_mir_bodies"], 4)
        del document["index"]["7"]
        with self.assertRaisesRegex(ValueError, "omits or invents source-derived"):
            reconcile(self.root, document, self.mir)

    def collection_fixture(self):
        for name in (*PACKAGE_FILES, *TOOL_FILES):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n")
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "jbig2-decoder"\nversion = "0.2.0-dev"\n',
        )
        (self.root / "config").mkdir()
        (self.root / "config/toolchain.json").write_text(json.dumps({
            "coverage": {"rust_toolchain": "nightly-2026-10-12"},
        }))
        output = self.root / "output"
        output.mkdir()
        (output / "compiler-items.json").write_text(json.dumps(self.document))
        (output / "compiler-mir.txt").write_text(self.mir)
        (output / "native-library.rlib").write_bytes(b"fixture-codegen-receipt")
        (output / "rustc-version.stdout").write_text("rustc fixture")
        (output / "rustdoc-version.stdout").write_text("rustdoc fixture")
        report = reconcile(self.root, self.document, self.mir)
        report.update(
            source_bindings=bindings(self.root),
            profile={
                "toolchain_source": "config/toolchain.json coverage.rust_toolchain",
                "toolchain": "nightly-2026-10-12", "package": "jbig2-decoder",
                "version": "0.2.0-dev", "commands": commands("nightly-2026-10-12"),
                "production_library": True, "tests_enabled": False, "verus_keep_ghost": False,
                "dependencies": [], "features": [], "isolated_cargo_home": True,
                "rustc_identity": "rustc fixture", "rustdoc_identity": "rustdoc fixture",
            },
            artifacts={name: {
                "sha256": digest(output / name), "bytes": (output / name).stat().st_size,
            } for name in ("compiler-items.json", "compiler-mir.txt", "native-library.rlib")},
        )
        (output / "report.json").write_text(json.dumps(report))
        return output, report

    def test_collection_replays_original_artifacts_without_granting_readiness(self):
        output, report = self.collection_fixture()
        self.assertEqual(validate_existing(self.root, output), report)
        self.assertFalse(report["ready_for_external_validation"])

    def test_rehashed_false_readiness_or_truncated_inventory_is_rejected(self):
        output, report = self.collection_fixture()
        for mutate in (
            lambda value: value.update(ready_for_external_validation=True),
            lambda value: value.update(complete_production_inventory_qualified=True),
            lambda value: value["records"].pop(),
            lambda value: value["counts"].update(executable_mir_bodies=0),
            lambda value: value["api_declaration_surfaces"].update(call_graph_reachability_qualified=True),
            lambda value: value["api_declaration_surfaces"]["categories"].update(root_reexported_function=99),
            lambda value: value["records"][0]["declaration_surface"].update(category="invented"),
        ):
            with self.subTest(mutation=mutate):
                changed = copy.deepcopy(report)
                mutate(changed)
                (output / "report.json").write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, "claims differ"):
                    validate_existing(self.root, output)

    def test_rehashed_missing_actual_body_cannot_be_hidden_by_new_artifact_hashes(self):
        output, report = self.collection_fixture()
        path = output / "compiler-mir.txt"
        path.write_text(path.read_text().replace(self.clone, ""))
        report["artifacts"][path.name] = {"sha256": digest(path), "bytes": path.stat().st_size}
        (output / "report.json").write_text(json.dumps(report))
        with self.assertRaisesRegex(ValueError, "body inventory"):
            validate_existing(self.root, output)

    def test_collection_rejects_stale_sources_and_test_or_ghost_profiles(self):
        output, report = self.collection_fixture()
        for field in ("tests_enabled", "verus_keep_ghost"):
            changed = copy.deepcopy(report)
            changed["profile"][field] = True
            (output / "report.json").write_text(json.dumps(changed))
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "build profile"):
                validate_existing(self.root, output)
        (output / "report.json").write_text(json.dumps(report))
        (self.root / TOOL_FILES[0]).write_text("changed actual producer\n")
        with self.assertRaisesRegex(ValueError, "source/tool bindings"):
            validate_existing(self.root, output)


if __name__ == "__main__":
    unittest.main()
