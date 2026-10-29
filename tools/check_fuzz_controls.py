#!/usr/bin/env python3
"""Validate real fuzz failure capture/replay in an isolated generated fixture."""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

from run_fuzz import ROOT, run, source_identity


MARKER = b"generated-fuzz-control-v1"
FIXTURE = """#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|input: &[u8]| {
    assert_ne!(input, b"generated-fuzz-control-v1", "synthetic harness failure control");
    let _ = jbig2_decoder::inspect_file_header(input);
});
"""


def arguments(mode: str, output: Path, corpus: Path, artifact=None):
    return SimpleNamespace(
        mode=mode, profile="smoke", targets=["file_header_arbitrary"],
        output_dir=output, corpus_dir=corpus, artifact=artifact,
    )


def check_controls(output: Path) -> dict:
    output.mkdir(parents=True, exist_ok=False)
    original = (ROOT / "fuzz/fuzz_targets/file_header_arbitrary.rs").read_bytes()
    original_identity = source_identity(ROOT)
    with tempfile.TemporaryDirectory(prefix="jbig2-fuzz-control-") as temporary:
        directory = Path(temporary).resolve() / "source"
        subprocess.run(
            ["git", "clone", "--quiet", "--no-local", "--single-branch", str(ROOT), str(directory)],
            check=True,
        )
        for name in ("src", "fuzz/fuzz_targets"):
            shutil.copytree(ROOT / name, directory / name, dirs_exist_ok=True)
        files = [
            "Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE",
            "fuzz/Cargo.toml", "fuzz/Cargo.lock",
            "config/toolchain.json", "validation/runtime_access.rs", "examples/fuzz_seeds.rs",
            "tools/distribution_consumer.rs", "tools/run_fuzz.py",
            "tools/maintain_fuzz_corpus.py", "tools/check_fuzz_controls.py",
            ".github/workflows/fuzz.yml",
            "tests/support/context_scenarios.rs", "benchmarks/generated.rs",
            "tests/support/codec_scenarios.rs",
            "fuzz/Cargo.toml", "fuzz/Cargo.lock",
        ]
        files += [str(path.relative_to(ROOT)) for path in (ROOT / "fuzz/fuzz_targets").glob("*.rs")]
        for name in files:
            path = directory / name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, path)
        if source_identity(directory) != original_identity:
            raise ValueError("isolated fuzz control copy does not match production source identity")
        corpus = output / "corpus"
        target_corpus = corpus / "file_header_arbitrary"
        target_corpus.mkdir(parents=True)
        seed = target_corpus / "generated-control"
        seed.write_bytes(MARKER)
        baseline = run(arguments("replay", output / "baseline", corpus, seed), directory)
        if baseline["status"] != "passed" or len(baseline["targets"]) != 1:
            raise ValueError("baseline single-input fuzz replay failed")
        target = directory / "fuzz/fuzz_targets/file_header_arbitrary.rs"
        target.write_text(FIXTURE, encoding="utf-8")
        campaign = run(arguments("run", output / "failure", corpus), directory)
        if campaign["status"] != "failed" or len(campaign["targets"]) != 1:
            raise ValueError("controlled fuzz failure did not execute exactly one target")
        outcome = campaign["targets"][0]
        if outcome["exit_code"] == 0 or not outcome["artifacts"]:
            raise ValueError("controlled fuzz failure did not capture a reproducer")
        artifact = output / "failure/file_header_arbitrary" / outcome["artifacts"][0]["name"]
        if artifact.read_bytes() != MARKER:
            raise ValueError("captured fuzz reproducer differs from the generated control")
        replay = run(arguments("replay", output / "reproduced", corpus, artifact), directory)
        if replay["status"] != "failed" or len(replay["targets"]) != 1:
            raise ValueError("generated control did not reproduce its failure")
        log = (output / "reproduced/file_header_arbitrary.log").read_text()
        if "synthetic harness failure control" not in log or "Running:" not in log:
            raise ValueError("replay failed without executing the expected generated control")
        target.write_bytes(original)
        restored = run(arguments("replay", output / "restored", corpus, artifact), directory)
        if restored["status"] != "passed" or len(restored["targets"]) != 1:
            raise ValueError("restored production target did not pass reproducer replay")
        if (
            baseline["source_sha256"] != original_identity
            or restored["source_sha256"] != original_identity
            or campaign["source_sha256"] == original_identity
            or replay["source_sha256"] != campaign["source_sha256"]
        ):
            raise ValueError("isolated failure/replay source identities do not match expected transitions")
    if (
        (ROOT / "fuzz/fuzz_targets/file_header_arbitrary.rs").read_bytes() != original
        or source_identity(ROOT) != original_identity
    ):
        raise ValueError("production fuzz target changed during isolated control")
    return {
        "schema_version": 1, "status": "passed", "production_sources_modified": False,
        "baseline_single_input": "passed", "generated_failure": "captured",
        "failure_replay": "reproduced", "restored_target_replay": "passed",
        "reproducer_sha256": hashlib.sha256(MARKER).hexdigest(),
        "production_source_sha256": original_identity,
        "baseline_source_sha256": baseline["source_sha256"],
        "failure_source_sha256": campaign["source_sha256"],
        "replay_source_sha256": replay["source_sha256"],
        "restored_source_sha256": restored["source_sha256"],
        "fixture_provenance": "project-generated constant and assertion; no official/private inputs",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    arguments_ = parser.parse_args()
    output = arguments_.output_dir.resolve()
    if output.exists():
        print("fuzz controls require a fresh output directory", file=sys.stderr)
        return 1
    try:
        result = check_controls(output)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"fuzz control validation failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    if not output.exists():
        output.mkdir(parents=True)
    (output / "control-report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
