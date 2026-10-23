#!/usr/bin/env python3
"""Validate repository workflow policy, not upstream action code or branch rules."""

import argparse
import hashlib
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
ACTION = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*@[0-9a-f]{40}")
TOOLCHAIN = re.compile(r"(?:[0-9]+\.[0-9]+\.[0-9]+|nightly-[0-9]{4}-[0-9]{2}-[0-9]{2})")
FUZZ_TRUST = (
    "(github.event_name == 'schedule' || github.event_name == 'workflow_dispatch')"
    " && github.ref == 'refs/heads/main' && github.event.repository.default_branch == 'main'"
)
FUZZ_PRODUCER_GROUP = (
    "${{ " + FUZZ_TRUST
    + " && format('fuzz-generated-v2-trusted-{0}-main', github.repository)"
    " || format('fuzz-generated-v2-untrusted-{0}-{1}-{2}',"
    " github.repository, github.run_id, github.run_attempt) }}"
)
VALIDATION_WORKFLOWS = {"ci", "coverage", "fuzz", "performance", "verus"}
DISABLED_VERUS_CHECK = re.compile(
    r"(?<![\w-])--(?:no-verify|no-lifetime|no-erasure-check|no-trait-conflicts"
    r"|no-auto-recommends-check)(?![\w-])"
)


class UniqueLoader(yaml.BaseLoader):
    """Keep GitHub's `on` key as a string without YAML 1.1 boolean coercion."""

    def construct_object(self, node, deep=False):
        if node.tag not in {
            yaml.resolver.BaseResolver.DEFAULT_SCALAR_TAG,
            yaml.resolver.BaseResolver.DEFAULT_SEQUENCE_TAG,
            yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG,
        }:
            raise ValueError(f"unsupported explicit YAML tag: {node.tag}")
        return super().construct_object(node, deep=deep)


def mapping(loader, node):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=True)
        if not isinstance(key, str) or key in result or key == "<<":
            raise ValueError(f"duplicate, merged or non-scalar YAML key: {key!r}")
        result[key] = loader.construct_object(value_node, deep=True)
    return result


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, mapping)


def load_workflow(text):
    document = yaml.load(text, Loader=UniqueLoader)
    if not isinstance(document, dict) or not document:
        raise ValueError("workflow/configuration must be a nonempty mapping")
    return document


def read_permissions(permissions):
    if not isinstance(permissions, dict) or set(permissions) - {"contents"}:
        raise ValueError("only contents read/none permissions are allowed by current CI policy")
    if any(not isinstance(value, str) or value not in {"read", "none"} for value in permissions.values()):
        raise ValueError("workflow permissions must not grant writes")


def selected_toolchain(step, job):
    settings = step.get("with", {})
    value = settings.get("toolchain") if isinstance(settings, dict) else None
    if not isinstance(value, str):
        raise ValueError("Rust setup requires an explicit toolchain input")
    if TOOLCHAIN.fullmatch(value):
        return value
    if re.fullmatch(r"\$\{\{\s*matrix\.toolchain\s*\}\}", value):
        strategy = job.get("strategy", {})
        matrix = strategy.get("matrix", {}) if isinstance(strategy, dict) else {}
        versions = matrix.get("toolchain") if isinstance(matrix, dict) else None
        if isinstance(versions, list) and versions and all(
            isinstance(version, str) and (version == "stable" or TOOLCHAIN.fullmatch(version))
            for version in versions
        ):
            return value
    raise ValueError("Rust setup requires a fixed version/dated nightly or reviewed stable matrix")


def check_fuzz_producer_serialization(document):
    jobs = document.get("jobs", {})
    producer = jobs.get("fuzz")
    if not isinstance(producer, dict) or not isinstance(producer.get("env"), dict):
        raise ValueError("fuzz corpus producer and exact trust predicate are required")
    if producer["env"].get("TRUSTED_CORPUS") != "${{ " + FUZZ_TRUST + " }}":
        raise ValueError("fuzz corpus trust predicate must remain unchanged")
    if producer.get("concurrency") != {
        "group": FUZZ_PRODUCER_GROUP, "cancel-in-progress": "false",
    }:
        raise ValueError("fuzz corpus producer serialization or untrusted isolation changed")
    if "concurrency" in document or any(
        isinstance(job, dict) and "concurrency" in job
        for name, job in jobs.items() if name != "fuzz"
    ):
        raise ValueError("fuzz concurrency belongs only to the corpus producer job")


def check_verus_sharding(document):
    jobs = document.get("jobs", {})
    if set(jobs) != {"whole", "semantic", "verify"} or not all(isinstance(job, dict) for job in jobs.values()):
        raise ValueError("Verus requires whole, semantic and stable verify collector jobs")
    whole, semantic, verify = (jobs[name] for name in ("whole", "semantic", "verify"))
    if any(job.get("timeout-minutes") != "20" for job in jobs.values()):
        raise ValueError("Verus job caps must remain 20 minutes")
    if (
        semantic.get("needs") != "whole"
        or semantic.get("strategy") != {
            "fail-fast": "false", "max-parallel": "4",
            "matrix": "${{ fromJSON(needs.whole.outputs.matrix) }}",
        }
        or whole.get("outputs") != {"matrix": "${{ steps.groups.outputs.matrix }}"}
        or verify.get("if") != "always()" or verify.get("needs") != ["whole", "semantic"]
        or "name" in verify
    ):
        raise ValueError("Verus dynamic complete matrix/stable fail-closed verify contract changed")
    whole_steps, semantic_steps, verify_steps = (
        job.get("steps", []) for job in (whole, semantic, verify)
    )
    setups = [
        [{key: step[key] for key in ("uses", "with") if key in step}
         for step in steps if step.get("uses", "").split("@")[0] in {
             "actions/checkout", "actions/setup-python", "dtolnay/rust-toolchain",
         }]
        for steps in (whole_steps, semantic_steps)
    ]
    downloads = [
        [step for step in steps if step.get("name") == "Download pinned Verus"]
        for steps in (whole_steps, semantic_steps)
    ]
    if setups[0] != setups[1] or len(setups[0]) != 3 or downloads[0] != downloads[1] or len(downloads[0]) != 1:
        raise ValueError("whole and semantic jobs must use identical pinned tool setup")
    pins = json.loads((ROOT / "toolchain.json").read_text())["verus"]
    download_run = downloads[0][0].get("run", "").replace("\\\n", " ")
    download_run = re.sub(r"\|\s*\n\s*", "| ", download_run)
    download_commands = [
        shlex.split(line, comments=True)
        for line in download_run.splitlines()
        if line.strip()
    ]
    if download_commands != [
        ["curl", "--fail", "--location", "--retry", "3", "--retry-all-errors",
         "--retry-delay", "2", "--retry-max-time", "60", "--connect-timeout", "10",
         "--max-time", "30", "--output", "verus.zip", pins["linux_archive_url"]],
        ["echo", f"{pins['linux_archive_sha256']}  verus.zip", "|", "sha256sum", "--check"],
        ["unzip", "-q", "verus.zip"],
    ]:
        raise ValueError("pinned compiler download requires bounded retries and integrity verification")
    runs = ["\n".join(step.get("run", "") for step in steps) for steps in (
        whole_steps, semantic_steps, verify_steps,
    )]
    commands = [
        [shlex.split(line, comments=True) for line in text.replace("\\\n", " ").splitlines()]
        for text in runs
    ]
    environments = [
        document.get("env", {}), *(job.get("env", {}) for job in jobs.values()),
        *(step.get("env", {}) for steps in (whole_steps, semantic_steps, verify_steps) for step in steps),
    ]
    if not all(isinstance(environment, dict) for environment in environments):
        raise ValueError("Verus validation environments must be mappings")
    if any(
        DISABLED_VERUS_CHECK.search(token)
        for command_list in commands for command in command_list for token in command
    ) or any(
        DISABLED_VERUS_CHECK.search(str(value))
        for environment in environments for value in environment.values()
    ):
        raise ValueError("Verus check-disabling flags are forbidden in validation commands/environment")
    cargo_invocations = [command for command in commands[0] if command[:2] == ["cargo", "verus"]]
    if cargo_invocations != [[
        "cargo", "verus", "build", "--manifest-path", "verification/Cargo.toml",
        "--locked", "--offline", "--", "--num-threads", "4", "--rlimit", "30",
        "--triggers-mode", "silent", "--output-json", ">",
        "target/verus-production/cargo-reports.jsons",
    ]]:
        raise ValueError("whole-crate gate requires the exact pinned Cargo verification-and-build invocation")
    for text in (
        "--require-complete",
        "diff -u production-proof-coverage.json", "tools/check_traceability.py",
        "--seal-whole target/semantic-ci/whole", "tools/run_semantic_proofs.py --matrix",
    ):
        if text not in runs[0]:
            raise ValueError(f"whole-crate verification gate missing: {text}")
    if '--shard "$SEMANTIC_SHARD"' not in runs[1] or not any(
        step.get("env") == {"SEMANTIC_SHARD": "${{ matrix.shard }}"} for step in semantic_steps
    ):
        raise ValueError("semantic matrix must execute its exact complete selected shard")
    download = [step for step in verify_steps if step.get("uses", "").startswith("actions/download-artifact@")]
    if len(download) != 1 or download[0].get("with") != {
        "pattern": "verus-partial-${{ github.run_id }}-${{ github.run_attempt }}-*",
        "path": "target/semantic-inputs", "merge-multiple": "false",
    } or download[0].get("uses") != "actions/download-artifact@37930b1c2abaa49bbe596cd826c3c89aef350131":
        raise ValueError("collector must download unmerged, pinned, current-run/current-attempt artifacts")
    if not any(
        step.get("env") == {
            "WHOLE_RESULT": "${{ needs.whole.result }}", "SEMANTIC_RESULT": "${{ needs.semantic.result }}",
        } and all(text in step.get("run", "") for text in (
            "--aggregate target/semantic-inputs",
            '--whole-result "$WHOLE_RESULT" --semantic-result "$SEMANTIC_RESULT"',
            "--output-dir target/semantic-canonical",
        )) for step in verify_steps
    ):
        raise ValueError("collector must check both upstream results and all evidence")
    uploads = {}
    for name, steps in (("whole", whole_steps), ("semantic", semantic_steps), ("verify", verify_steps)):
        matches = [step for step in steps if step.get("uses", "").startswith("actions/upload-artifact@")]
        if len(matches) != 1 or matches[0].get("if") != "always()":
            raise ValueError("each Verus job must retain failed diagnostics")
        uploads[name] = matches[0].get("with")
        if any(
            "if" in step for step in steps
            if step.get("run") and step.get("name") != "Stage whole-crate outputs including failed diagnostics"
        ):
            raise ValueError("Verus evidence-producing/collecting steps must not be conditional")
    if uploads != {
        "whole": {"name": "verus-partial-${{ github.run_id }}-${{ github.run_attempt }}-whole",
                  "path": "target/semantic-ci/whole/"},
        "semantic": {"name": "verus-partial-${{ github.run_id }}-${{ github.run_attempt }}-shard-${{ matrix.shard }}",
                     "path": "target/semantic-ci/group/"},
        "verify": {"name": "verus-bootstrap-log", "path": "target/semantic-canonical/"},
    }:
        raise ValueError("Verus immutable partial/canonical artifact names or layout changed")


def check_workflow(document, name):
    if document.get("permissions") != {"contents": "read"}:
        raise ValueError(f"{name}: workflow must default to contents: read only")
    events = document.get("on")
    if not isinstance(events, (dict, list, str)) or not events or "pull_request_target" in events:
        raise ValueError(f"{name}: missing or unsupported privileged workflow trigger")
    if isinstance(events, list) and any(not isinstance(event, str) for event in events):
        raise ValueError(f"{name}: workflow event names must be strings")
    if Path(name).stem in VALIDATION_WORKFLOWS and (
        not isinstance(events, dict) or "pull_request" not in events
        or events.get("pull_request") not in ("", None, {})
        or "workflow_dispatch" not in events
        or events.get("push") != {"branches": ["main"], "tags": ["**"]}
    ):
        raise ValueError("validation runs on every PR, main/tag pushes and manual dispatch, not duplicate branch pushes")
    jobs = document.get("jobs")
    if not isinstance(jobs, dict) or not jobs:
        raise ValueError(f"{name}: workflow must contain jobs")
    if name in {"fuzz.yml", "fuzz.yaml"}:
        check_fuzz_producer_serialization(document)
    if name in {"verus.yml", "verus.yaml"}:
        check_verus_sharding(document)
    actions = []
    for job_name, job in jobs.items():
        if not isinstance(job, dict):
            raise ValueError(f"{name}/{job_name}: job must be a mapping")
        if "permissions" in job:
            read_permissions(job["permissions"])
        timeout = job.get("timeout-minutes")
        if not isinstance(timeout, str) or not timeout.isascii() or not timeout.isdecimal() or not 1 <= int(timeout) <= 60:
            raise ValueError(f"{name}/{job_name}: explicit timeout must be within 1..60 minutes")
        steps = job.get("steps")
        if "uses" in job or not isinstance(steps, list) or not steps:
            raise ValueError(f"{name}/{job_name}: explicit steps required; reusable jobs need policy review")
        if job.get("continue-on-error", "false") != "false":
            raise ValueError(f"{name}/{job_name}: failures must not be masked")
        for step in steps:
            if not isinstance(step, dict):
                raise ValueError(f"{name}/{job_name}: step must be a mapping")
            if step.get("continue-on-error", "false") != "false" or "permissions" in step:
                raise ValueError(f"{name}/{job_name}: step failure/permission override is unsupported")
            if "uses" not in step:
                continue
            reference = step["uses"]
            if not isinstance(reference, str) or not ACTION.fullmatch(reference):
                raise ValueError(f"{name}/{job_name}: action requires a full commit SHA: {reference}")
            entry = {"workflow": name, "job": job_name, "uses": reference}
            if reference.split("@")[0] == "dtolnay/rust-toolchain":
                entry["toolchain"] = selected_toolchain(step, job)
            actions.append(entry)
    if not actions:
        raise ValueError(f"{name}: no pinned actions were checked")
    return actions


def check_updates(document):
    updates = document.get("updates")
    if document.get("version") != "2" or not isinstance(updates, list):
        raise ValueError("Dependabot version 2 action-update policy is required")
    matching = [
        entry for entry in updates if isinstance(entry, dict)
        and entry.get("package-ecosystem") == "github-actions" and entry.get("directory") == "/"
    ]
    if len(matching) != 1 or matching[0].get("schedule") != {"interval": "weekly"}:
        raise ValueError("exactly one weekly root GitHub Actions update entry is required")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    try:
        arguments.output.unlink(missing_ok=True)
        directory = ROOT / ".github/workflows"
        paths = sorted([*directory.glob("*.yml"), *directory.glob("*.yaml")])
        if not paths:
            raise ValueError("no workflow files were found")
        actions = []
        bindings = {}
        for path in paths:
            actions.extend(check_workflow(load_workflow(path.read_text()), path.name))
            bindings[path.relative_to(ROOT).as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
        policy = ROOT / ".github/dependabot.yml"
        check_updates(load_workflow(policy.read_text()))
        bindings[policy.relative_to(ROOT).as_posix()] = hashlib.sha256(policy.read_bytes()).hexdigest()
        report = {
            "schema_version": 1, "status": "workflow_policy_passed",
            "workflow_count": len(paths), "action_count": len(actions), "actions": actions,
            "source_bindings": bindings,
            "source_revision": subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True,
            ).stdout.strip(),
            "note": "Checks configuration pins, explicit toolchains, read-only permissions, job budgets, bounded complete Verus sharding/fail-closed collection, fuzz producer serialization/isolation and update configuration. Not upstream code approval, protected-branch enforcement, trigger execution or full readiness evidence.",
        }
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({key: report[key] for key in ("status", "workflow_count", "action_count")}))
        return 0
    except (OSError, ValueError, yaml.YAMLError, subprocess.CalledProcessError) as error:
        print(f"workflow policy validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
