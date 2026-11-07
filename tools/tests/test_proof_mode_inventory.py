import copy
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

import collect_proof_mode_inventory as collector
from check_traceability import runtime_inventory
from proof_mode_inventory import PREFIX, fields, lexical_module, parse_form, project_functions, reconcile
import run_semantic_proofs as runner
import check_workflows


def function(path, line=4, mode="Exec", kind="(FunctionKind Static)", parameters=""):
    location = f"../src/lib.rs:{line}:1: {line}:14 (#0)"
    return (
        f"(@ {json.dumps(location)} (Function :name (Fun :path {path}) :proxy None :kind {kind} "
        f":mode {mode} :owning_module jbig2_rs_production:: :typ_params () :typ_bounds () :params ({parameters}) "
        f":ret (@ {json.dumps(location)} (Param :name (VarIdent \"%return\") :typ (Typ Bool))) "
        ":require () :ensure (tuple () ()) :returns None :decrease () "
        ":attrs (FunctionAttrs :is_external_body false) :body (Body)))"
    )


class ProofModeInventoryTests(unittest.TestCase):
    def test_lexical_implementation_scope_is_distinct_from_canonical_type_owner_scope(self):
        source = {
            "file": "src/helpers/construct.rs", "file_module": "helpers::construct",
            "id": "helpers::construct::Token::new", "owner": "Token", "name": "new",
            "verifier_module": "datatype_owner",
        }
        self.assertEqual(lexical_module(source), "helpers::construct")
        source.update(file="src/lib.rs", file_module="", id="lib::nested::keep",
                      owner=None, name="keep")
        self.assertEqual(lexical_module(source), "nested")

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        (self.root / "src").mkdir()
        (self.root / "src/lib.rs").write_text(
            "#[derive(Clone, Copy)]\nstruct Token(u8);\n"
            "#[cfg_attr(verus_keep_ghost, verus_verify)]\nfn keep() {}\n",
        )
        runtime = runtime_inventory(self.root)
        self.identity, self.source = next(iter(runtime.items()))
        self.keep = function(PREFIX + "keep")
        self.clone = function(
            PREFIX + "impl&%0::clone", line=1,
            kind=f"(FunctionKind TraitMethodImpl :trait_path core::clone::Clone "
                 f":trait_typ_args ((Typ Datatype (Dt Path {PREFIX}Token) () ())))",
        ).replace("lib.rs:1:1: 1:14", "lib.rs:1:10: 1:15")
        self.helper = function(PREFIX + "helper", mode="Proof")
        self.dependency = function("vstd::dependency", mode="Spec")
        self.text = "\n\n".join((self.dependency, self.keep, self.clone, self.helper)) + "\n"
        self.coverage = {
            "schema_version": 1, "source": "src/**/*.rs", "verus_verified_obligations": 3,
            "native_assumptions": [],
            "counts": {"verified": 1, "reviewed-verifier-limitation": 0, "audited-forwarder": 0,
                       "runtime-proof-divergence": 0, "native-assumption": 0, "incomplete": 0},
            "functions": [{
                "id": self.identity, "file": self.source["file"], "visibility": "private",
                "file_sha256": self.source["file_sha256"], "status": "verified",
                "disposition_reason": None, "verus_functions": [PREFIX + "keep"],
            }],
        }
        self.whole = {
            "func-details": {PREFIX + name: {"obligation_proof_notes": [], "failed_proof_notes": []}
                             for name in ("keep", "Token::clone", "helper")},
            "verification-results": {
                "success": True, "verified": 3, "errors": 0,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
            "verus": {"version": "pinned", "commit": "a" * 40, "toolchain": "1.98.1-fixture"},
        }
        self.native = {
            "records": [{
                "id": self.identity, "source_id": self.identity, "name": "keep",
                "origin": "explicit_source_declaration",
                "source_span": {"filename": "src/lib.rs", "begin": [4, 1], "end": [4, 14]},
                "impl_trait": None,
            }, {
                "id": "derive:src/lib.rs:1:10:clone", "source_id": None, "name": "clone",
                "origin": "compiler_derived_method", "impl_trait": "Clone",
                "source_span": {"filename": "src/lib.rs", "begin": [1, 10], "end": [1, 15]},
            }, {
                "id": "derive:src/lib.rs:1:17:fmt", "source_id": None, "name": "fmt",
                "origin": "compiler_derived_method", "impl_trait": "Debug",
                "source_span": {"filename": "src/lib.rs", "begin": [1, 17], "end": [1, 22]},
            }],
        }

    def test_exact_expanded_identity_mode_clone_and_native_only_accounting(self):
        report = reconcile(self.root, self.text, self.native, self.whole, self.coverage)
        self.assertEqual(report["counts"], {
            "project_functions": 3, "modes": {"Exec": 2, "Proof": 1},
            "matched_verified_explicit": 1, "matched_source_clones": 1,
            "native_function_items_not_in_expanded_exec": 1, "whole_project_names_bound": 3,
        })
        self.assertTrue(report["native_profile_reconciled"])
        self.assertFalse(report["ready_for_external_validation"])
        self.assertFalse(report["generated_contract_quality_qualified"])
        self.assertEqual(report["native_function_items_not_in_expanded_exec"][0]["impl_trait"], "Debug")
        self.assertEqual({row["canonical_verifier_name"] for row in report["records"]},
                         set(self.whole["func-details"]))

    def test_without_native_export_does_not_manufacture_native_reconciliation(self):
        report = reconcile(self.root, self.text, None, self.whole, self.coverage)
        self.assertFalse(report["native_profile_reconciled"])
        self.assertIsNone(report["counts"]["native_function_items_not_in_expanded_exec"])
        self.assertEqual(report["counts"]["matched_source_clones"], 1)

    def test_truncated_unbalanced_ambiguous_and_unparsed_streams_fail(self):
        for text in (self.text[:-3], self.keep + ")", '(@ "unterminated', "(X :name 1 :name 2)"):
            with self.subTest(text=text[:30]), self.assertRaises(ValueError):
                if text.startswith("(X"):
                    fields(parse_form(text))
                else:
                    project_functions(self.root, text)

    def test_omitted_explicit_derive_or_whole_helper_cannot_be_rehashed_away(self):
        for omitted in (self.keep, self.clone, self.helper):
            with self.subTest(omitted=omitted[:60]), self.assertRaisesRegex(ValueError, "omits"):
                reconcile(self.root, self.text.replace(omitted, ""), self.native,
                          self.whole, self.coverage)

    def test_duplicate_or_unmapped_executables_and_wrong_modes_are_rejected(self):
        for text in (
            self.text + "\n\n" + self.keep,
            self.text + "\n\n" + function(PREFIX + "unmapped"),
            self.text.replace(":mode Exec", ":mode Spec", 1),
            self.text.replace(":mode Exec", ":mode Unknown", 1),
        ):
            with self.subTest(text=text[-60:]), self.assertRaises(ValueError):
                reconcile(self.root, text, self.native, self.whole, self.coverage)

    def test_external_body_and_missing_source_owner_are_rejected(self):
        for text in (
            self.text.replace(":is_external_body false", ":is_external_body true"),
            self.text.replace("../src/lib.rs", "../../outside.rs"),
            self.text.replace(":body (Body)", ":body None"),
            self.text.replace(":owning_module jbig2_rs_production::",
                              ":owning_module jbig2_rs_production::wrong"),
        ):
            with self.subTest(text=text[:60]), self.assertRaises(ValueError):
                reconcile(self.root, text, self.native, self.whole, self.coverage)

    def test_partial_failure_and_stale_or_incomplete_source_accounting_are_rejected(self):
        for field, value in (
            ("success", False), ("errors", 1), ("is-verifying-entire-crate", False),
        ):
            whole = copy.deepcopy(self.whole)
            whole["verification-results"][field] = value
            with self.assertRaises(ValueError):
                reconcile(self.root, self.text, self.native, whole, self.coverage)
        for modify in (
            lambda value: value["functions"].clear(),
            lambda value: value["functions"].append(value["functions"][0]),
            lambda value: value["functions"][0].update(file_sha256="0" * 64),
        ):
            coverage = copy.deepcopy(self.coverage)
            modify(coverage)
            with self.assertRaises(ValueError):
                reconcile(self.root, self.text, self.native, self.whole, coverage)

    def fixture(self):
        for name in collector.TOOLS:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture\n")
        pins = {"version": "pinned", "commit": "a" * 40, "rust_toolchain": "1.98.1"}
        (self.root / "config/toolchain.json").write_text(json.dumps({"verus": pins}))
        coverage = self.root / "verification/production-proof-coverage.json"
        coverage.write_text(json.dumps(self.coverage))
        whole = self.root / "whole.json"
        whole.write_text(json.dumps(self.whole))
        raw = self.root / "crate.vir"
        raw.write_text(self.text)
        library = self.root / "library.rlib"
        library.write_bytes(b"fixture-original-native-library")
        output = self.root / "expanded"
        return raw, whole, coverage, library, output

    def test_original_project_only_collection_and_replay_keep_dependency_bodies_out(self):
        raw, whole, coverage, library, output = self.fixture()
        report = collector.collect(self.root, raw, whole, coverage, library, output)
        self.assertNotIn("vstd::dependency", (output / "project-functions.vir").read_text())
        self.assertEqual(collector.validate_existing(self.root, output, whole, coverage, library), report)
        with patch.object(collector, "validate_native", return_value=self.native):
            compared = collector.validate_existing(self.root, output, whole, coverage, library,
                                                   self.root / "native")
        self.assertTrue(compared["native_profile_reconciled"])

    def test_replay_rejects_rehashed_omissions_counts_and_false_qualification(self):
        raw, whole, coverage, library, output = self.fixture()
        report = collector.collect(self.root, raw, whole, coverage, library, output)
        for modify in (
            lambda value: value.update(ready_for_external_validation=True),
            lambda value: value.update(native_profile_reconciled=True),
            lambda value: value["records"].pop(),
            lambda value: value["counts"].update(matched_source_clones=0),
        ):
            changed = copy.deepcopy(report)
            modify(changed)
            (output / "report.json").write_text(json.dumps(changed))
            with self.assertRaisesRegex(ValueError, "claims differ"):
                collector.validate_existing(self.root, output, whole, coverage, library)

    def test_raw_export_omission_remains_rejected_after_artifact_hash_refresh(self):
        raw, whole, coverage, library, output = self.fixture()
        report = collector.collect(self.root, raw, whole, coverage, library, output)
        export = output / "project-functions.vir"
        export.write_text(export.read_text().replace(self.clone, ""))
        report["artifacts"]["project-functions.vir"] = {
            "sha256": collector.digest(export), "bytes": export.stat().st_size,
        }
        (output / "report.json").write_text(json.dumps(report))
        with self.assertRaisesRegex(ValueError, "omits"):
            collector.validate_existing(self.root, output, whole, coverage, library)

    def test_whole_boundary_requires_expanded_evidence_for_the_current_source_profile(self):
        raw, whole, coverage, library, output = self.fixture()
        directory = self.root / "whole"
        production = directory / "target/verus-production"
        production.mkdir(parents=True)
        shutil.copyfile(library, production / "libjbig2_rs_production.rlib")
        with self.assertRaises((OSError, ValueError)):
            runner.validate_expanded_inventory(self.root, directory)

    def test_workflow_requires_original_logging_and_source_bound_extraction(self):
        text = (TOOLS.parent / ".github/workflows/verus.yml").read_text()
        for changed in (
            text.replace("--log vir --log-dir target/verus-expanded-raw", ""),
            text.replace("python tools/collect_proof_mode_inventory.py", "python missing_collector.py"),
        ):
            with self.assertRaises(ValueError):
                check_workflows.check_verus_sharding(check_workflows.load_workflow(changed))


if __name__ == "__main__":
    unittest.main()
