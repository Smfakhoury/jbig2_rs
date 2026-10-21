import copy
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest.mock import patch


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_workflows import check_updates, check_workflow, load_workflow, main


class WorkflowPolicyTests(unittest.TestCase):
    def setUp(self):
        self.document = load_workflow(
            "name: generated policy fixture\n"
            "on: [pull_request, push]\n"
            "permissions: {contents: read}\n"
            "jobs:\n"
            "  checks:\n"
            "    runs-on: ubuntu-latest\n"
            "    timeout-minutes: 20\n"
            "    steps:\n"
            f"      - uses: owner/action@{'a' * 40}\n"
            "      - run: |\n"
            "          echo 'uses: owner/action@v4 is only text'\n"
        )

    def test_actual_yaml_actions_are_checked_without_matching_run_text(self):
        actions = check_workflow(self.document, "fixture.yml")
        self.assertEqual(len(actions), 1)
        self.assertEqual(actions[0]["uses"], "owner/action@" + "a" * 40)
        self.assertIn("on", self.document)
        self.assertNotIn(True, self.document)

    def test_empty_nonmapping_and_duplicate_key_yaml_is_rejected(self):
        for text in ("", "{}", "[]", "name: one\nname: two\n"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                load_workflow(text)

    def test_yaml_merges_and_custom_tags_cannot_hide_policy_fields(self):
        for text in (
            "base: &base {permissions: write-all}\n<<: *base\n",
            "!custom {permissions: read-all}\n",
        ):
            with self.subTest(text=text), self.assertRaises(ValueError):
                load_workflow(text)

    def test_floating_short_dynamic_local_and_container_uses_are_rejected(self):
        for reference in (
            "owner/action@v4", "owner/action@master", "owner/action@" + "a" * 7,
            "owner/action@${{ inputs.ref }}", "./.github/actions/local",
            "docker://example/image@sha256:" + "a" * 64,
        ):
            document = copy.deepcopy(self.document)
            document["jobs"]["checks"]["steps"][0]["uses"] = reference
            with self.subTest(reference=reference), self.assertRaisesRegex(ValueError, "full commit SHA"):
                check_workflow(document, "fixture.yml")

    def test_flow_style_actions_cannot_hide_a_floating_ref(self):
        document = load_workflow(
            "on: push\npermissions: {contents: read}\n"
            "jobs: {check: {runs-on: ubuntu-latest, timeout-minutes: 20, "
            "steps: [{uses: owner/action@v4}]}}\n"
        )
        with self.assertRaisesRegex(ValueError, "full commit SHA"):
            check_workflow(document, "flow.yml")

    def test_job_and_step_write_or_broad_permission_overrides_are_rejected(self):
        for permission in ("write-all", {"contents": "write"}, {"actions": "read"}, {"contents": []}):
            document = copy.deepcopy(self.document)
            document["jobs"]["checks"]["permissions"] = permission
            with self.subTest(permission=permission), self.assertRaises(ValueError):
                check_workflow(document, "fixture.yml")
        document = copy.deepcopy(self.document)
        document["permissions"] = {"contents": "write"}
        with self.assertRaises(ValueError):
            check_workflow(document, "fixture.yml")
        document = copy.deepcopy(self.document)
        document["jobs"]["checks"]["steps"][0]["permissions"] = {}
        with self.assertRaises(ValueError):
            check_workflow(document, "fixture.yml")

    def test_privileged_trigger_is_rejected_in_all_supported_yaml_shapes(self):
        for events in ("pull_request_target", ["pull_request_target"], {"pull_request_target": {}}):
            document = copy.deepcopy(self.document)
            document["on"] = events
            with self.subTest(events=events), self.assertRaises(ValueError):
                check_workflow(document, "fixture.yml")

    def test_job_budgets_are_explicit_and_bounded(self):
        for value in (None, "0", "61", "1.5", "${{ inputs.timeout }}"):
            document = copy.deepcopy(self.document)
            document["jobs"]["checks"]["timeout-minutes"] = value
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "timeout"):
                check_workflow(document, "fixture.yml")
        document = copy.deepcopy(self.document)
        document["jobs"]["checks"]["timeout-minutes"] = "60"
        self.assertTrue(check_workflow(document, "fixture.yml"))

    def test_failure_masking_and_unreviewed_reusable_jobs_are_rejected(self):
        for level in ("job", "step"):
            document = copy.deepcopy(self.document)
            job = document["jobs"]["checks"]
            target = job if level == "job" else job["steps"][0]
            target["continue-on-error"] = "true"
            with self.subTest(level=level), self.assertRaises(ValueError):
                check_workflow(document, "fixture.yml")
        document = copy.deepcopy(self.document)
        document["jobs"]["checks"]["uses"] = "owner/repo/workflow.yml@" + "a" * 40
        with self.assertRaises(ValueError):
            check_workflow(document, "fixture.yml")

    def test_rust_action_requires_an_explicit_fixed_or_dated_toolchain(self):
        for version in (None, "", "stable", "nightly", "1.89"):
            document = copy.deepcopy(self.document)
            step = document["jobs"]["checks"]["steps"][0]
            step["uses"] = "dtolnay/rust-toolchain@" + "a" * 40
            step["with"] = {"toolchain": version}
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, "toolchain|Rust setup"):
                check_workflow(document, "fixture.yml")
        for version in ("1.89.0", "nightly-2026-10-12"):
            document = copy.deepcopy(self.document)
            step = document["jobs"]["checks"]["steps"][0]
            step["uses"] = "dtolnay/rust-toolchain@" + "a" * 40
            step["with"] = {"toolchain": version}
            self.assertEqual(check_workflow(document, "fixture.yml")[0]["toolchain"], version)

    def test_stable_matrix_requires_an_explicit_reviewed_version_list(self):
        document = copy.deepcopy(self.document)
        job = document["jobs"]["checks"]
        job["steps"][0].update({
            "uses": "dtolnay/rust-toolchain@" + "a" * 40,
            "with": {"toolchain": "${{ matrix.toolchain }}"},
        })
        for versions in (None, [], ["nightly"], ["1.89.0", "stable"]):
            job["strategy"] = {"matrix": {"toolchain": versions}}
            if versions == ["1.89.0", "stable"]:
                self.assertTrue(check_workflow(document, "fixture.yml"))
            else:
                with self.subTest(versions=versions), self.assertRaises(ValueError):
                    check_workflow(document, "fixture.yml")

    def test_weekly_root_action_updates_are_configured_exactly_once(self):
        document = load_workflow(
            "version: 2\nupdates:\n"
            "  - package-ecosystem: github-actions\n    directory: /\n"
            "    schedule: {interval: weekly}\n"
        )
        check_updates(document)
        for change in ("missing", "duplicate", "wrong_interval"):
            changed = copy.deepcopy(document)
            if change == "missing":
                changed["updates"] = []
            elif change == "duplicate":
                changed["updates"] *= 2
            else:
                changed["updates"][0]["schedule"]["interval"] = "never"
            with self.subTest(change=change), self.assertRaises(ValueError):
                check_updates(changed)

    def test_repository_workflows_preserve_declared_toolchain_selections(self):
        root = TOOLS.parent
        toolchains = json.loads((root / "toolchain.json").read_text())
        selections = {}
        for path in sorted((root / ".github/workflows").glob("*.yml")):
            for entry in check_workflow(load_workflow(path.read_text()), path.name):
                if "toolchain" in entry:
                    selections[(path.name, entry["job"])] = entry["toolchain"]
        self.assertEqual(selections, {
            ("ci.yml", "cargo"): toolchains["cargo_rust_toolchain"],
            ("ci.yml", "distribution"): "${{ matrix.toolchain }}",
            ("coverage.yml", "coverage"): toolchains["coverage"]["rust_toolchain"],
            ("fuzz.yml", "controls"): toolchains["fuzz_rust_toolchain"],
            ("fuzz.yml", "fuzz"): toolchains["fuzz_rust_toolchain"],
            ("performance.yml", "performance"): toolchains["cargo_rust_toolchain"],
            ("verus.yml", "whole"): toolchains["verus"]["rust_toolchain"],
            ("verus.yml", "semantic"): toolchains["verus"]["rust_toolchain"],
        })
        check_updates(load_workflow((root / ".github/dependabot.yml").read_text()))

    def test_verus_stable_collector_and_bounded_dynamic_shards_are_policy_protected(self):
        actual = load_workflow((TOOLS.parent / ".github/workflows/verus.yml").read_text())
        self.assertTrue(check_workflow(actual, "verus.yml"))
        mutations = [
            lambda j: j["verify"].pop("if"),
            lambda j: j["verify"].update(needs=["semantic"]),
            lambda j: j["verify"].update(name="renamed"),
            lambda j: j["whole"].update(**{"timeout-minutes": "21"}),
            lambda j: j["semantic"]["strategy"].update(**{"fail-fast": "true"}),
            lambda j: j["semantic"]["strategy"].update(**{"max-parallel": "8"}),
            lambda j: j["semantic"]["strategy"].update(matrix={"group": ["reset"]}),
            lambda j: j["whole"].pop("outputs"),
            lambda j: j["semantic"]["steps"][3]["with"].update(toolchain="1.89.0"),
            lambda j: j["whole"]["steps"][7].update(run="echo skipped"),
            lambda j: j["verify"]["steps"][3]["with"].update(**{"merge-multiple": "true"}),
            lambda j: j["verify"]["steps"][3]["with"].update(**{"run-id": "previous"}),
            lambda j: j["verify"]["steps"][4].update(run="true"),
            lambda j: j["verify"]["steps"][4].update(**{"if": "false"}),
            lambda j: j["whole"]["steps"][-1]["with"].update(name="verus-bootstrap-log"),
            lambda j: j["semantic"]["steps"][-1].pop("if"),
        ]
        for mutate in mutations:
            document = copy.deepcopy(actual)
            mutate(document["jobs"])
            with self.subTest(mutate=mutate), self.assertRaises(ValueError):
                check_workflow(document, "verus.yml")

    def test_cli_failure_removes_prior_success_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory = root / ".github/workflows"
            directory.mkdir(parents=True)
            (directory / "bad.yml").write_text("permissions: write-all\n")
            output = root / "report.json"
            output.write_text('{"status":"workflow_policy_passed"}')
            with patch("check_workflows.ROOT", root), patch.object(
                sys, "argv", ["check_workflows", "--output", str(output)],
            ), redirect_stderr(io.StringIO()) as errors:
                self.assertEqual(main(), 1)
            self.assertIn("workflow policy validation failed", errors.getvalue())
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
