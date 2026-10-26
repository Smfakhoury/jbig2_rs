#!/usr/bin/env python3
"""Run bounded fuzz campaigns or deterministic single-input replay."""

import argparse
import hashlib
import json
import os
import signal
import stat
import subprocess
import sys
import time
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGETS = (
    "file_header_arbitrary", "file_header_structured", "document_arbitrary",
    "arithmetic", "huffman", "mmr", "generic_region",
    "document_context_structured",
    "codec_families_structured",
)
PROFILES = {"smoke": 60, "extended": 300}
INPUT_TIMEOUT = 5
RSS_LIMIT_MB = 1024
MAX_INPUT_BYTES = 16_384
MAX_CORPUS_FILES = 50_000
MAX_CORPUS_BYTES = 128 << 20
CAMPAIGN_STEP_SECONDS = 49 * 60
CAMPAIGN_SECONDS = CAMPAIGN_STEP_SECONDS - 5


def fuzz_environment() -> dict:
    excluded = {
        "RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTUP_TOOLCHAIN", "RUSTC", "RUSTDOC",
        "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTC_BOOTSTRAP",
        "CARGO_BUILD_RUSTFLAGS", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC",
        "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_TARGET", "CARGO_TARGET_DIR", "ASAN_OPTIONS", "LSAN_OPTIONS",
        "UBSAN_OPTIONS", "MSAN_OPTIONS", "TSAN_OPTIONS",
    }
    environment = {
        key: value for key, value in os.environ.items()
        if key not in excluded
        and not (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS"))
    }
    environment["CARGO_TERM_COLOR"] = "never"
    return environment


def source_identity(root: Path) -> str:
    paths = sorted((root / "src").rglob("*.rs"))
    paths += sorted((root / "fuzz/fuzz_targets").glob("*.rs"))
    paths += [root / name for name in (
        "Cargo.toml", "Cargo.lock", "fuzz/Cargo.toml", "fuzz/Cargo.lock",
        "validation/runtime_access.rs", "examples/fuzz_seeds.rs",
        "tools/distribution_consumer.rs", "tools/run_fuzz.py", "config/toolchain.json",
        "tests/support/context_scenarios.rs", "benchmarks/generated.rs",
        "tests/support/codec_scenarios.rs",
        "tools/maintain_fuzz_corpus.py", "tools/check_fuzz_controls.py",
        ".github/workflows/fuzz.yml",
    )]
    identity = hashlib.sha256()
    for path in paths:
        identity.update(path.relative_to(root).as_posix().encode() + b"\0")
        identity.update(path.read_bytes() + b"\0")
    return identity.hexdigest()


def validate_targets(root: Path, selected: list[str]) -> None:
    manifest = tomllib.loads((root / "fuzz/Cargo.toml").read_text())
    declared = [entry["name"] for entry in manifest["bin"]]
    if set(declared) != set(TARGETS) or len(declared) != len(TARGETS):
        raise ValueError("fuzz target manifest changed; review campaign coverage")
    if not selected or len(selected) != len(set(selected)) or set(selected) - set(TARGETS):
        raise ValueError("missing, duplicated or unknown fuzz target")


def inventory(directory: Path, max_size: int, max_files: int) -> list[dict]:
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError(f"fuzz inputs must be in a real directory: {directory}")
    result = []
    total = 0
    paths = []
    for path in directory.iterdir():
        if len(paths) >= max_files:
            raise ValueError(f"fuzz input collection exceeds reviewed bounds: {directory}")
        paths.append(path)
    for path in sorted(paths):
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f"non-regular fuzz input: {path}")
        size = metadata.st_size
        if size > max_size:
            raise ValueError(f"oversized fuzz input: {path}")
        total += size
        if total > MAX_CORPUS_BYTES or len(result) >= max_files:
            raise ValueError(f"fuzz input collection exceeds reviewed bounds: {directory}")
        descriptor = os.open(
            path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0),
        )
        with os.fdopen(descriptor, "rb") as stream:
            opened = os.fstat(stream.fileno())
            if not stat.S_ISREG(opened.st_mode):
                raise ValueError(f"non-regular opened fuzz input: {path}")
            contents = stream.read(max_size + 1)
        current = path.lstat()
        fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
        if (
            len(contents) != size
            or any(getattr(metadata, field) != getattr(opened, field)
                   or getattr(metadata, field) != getattr(current, field) for field in fields)
        ):
            raise ValueError(f"fuzz input changed during inventory: {path}")
        result.append({
            "name": path.name, "bytes": size,
            "sha256": hashlib.sha256(contents).hexdigest(),
        })
    return result


def corpus_summary(entries: list[dict]) -> dict:
    identity = hashlib.sha256()
    for entry in entries:
        identity.update(entry["name"].encode() + b"\0" + entry["sha256"].encode() + b"\0")
    return {
        "files": len(entries), "bytes": sum(entry["bytes"] for entry in entries),
        "sha256": identity.hexdigest(),
    }


def execute(command: list[str], root: Path, environment: dict, log: Path, timeout: int) -> dict:
    started = time.monotonic()
    timed_out = False
    with log.open("w", encoding="utf-8") as output:
        process = subprocess.Popen(
            command, cwd=root, env=environment, stdout=output, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            exit_code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            output.write("\nRunner deadline exceeded; terminating its process group.\n")
            output.flush()
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                output.write("Process group exited before termination was delivered.\n")
            try:
                exit_code = process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    output.write("Process group exited before the final termination signal.\n")
                exit_code = process.wait()
    return {
        "command": command, "exit_code": exit_code, "timed_out": timed_out,
        "seconds": time.monotonic() - started, "log": log.name,
    }


def fuzz_command(cargo: list[str], target: str, inputs: Path, artifacts: Path, seconds: int,
                 replay: bool = False) -> list[str]:
    limits = [
        f"-timeout={INPUT_TIMEOUT}", f"-rss_limit_mb={RSS_LIMIT_MB}",
        f"-max_len={MAX_INPUT_BYTES}", f"-artifact_prefix={artifacts}{os.sep}",
        "-print_final_stats=1",
    ]
    limits.append("-runs=1" if replay else f"-max_total_time={seconds}")
    return cargo + ["fuzz", "run", target, str(inputs), "--"] + limits


def execution_count(log: Path) -> int:
    prefix = "stat::number_of_executed_units:"
    with log.open(encoding="utf-8") as stream:
        values = [line[len(prefix):].strip() for line in stream if line.startswith(prefix)]
    if (
        len(values) != 1 or not values[0].isascii() or not values[0].isdecimal()
        or int(values[0]) < 1
    ):
        raise ValueError(f"missing, duplicated or invalid libFuzzer execution counter: {log}")
    return int(values[0])


def run(arguments, root: Path = ROOT) -> dict:
    if sys.platform != "linux":
        raise ValueError("the bounded fuzz runner currently requires Linux process groups")
    output = arguments.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    result = {"schema_version": 1, "status": "failed", "mode": arguments.mode, "targets": []}
    deadline = time.monotonic() + CAMPAIGN_SECONDS
    try:
        def bounded(command, log, seconds):
            remaining = int(deadline - time.monotonic()) - 5
            if remaining < 1:
                raise ValueError("fuzz campaign total deadline exhausted")
            return execute(command, root, environment, log, min(seconds, remaining))

        targets = arguments.targets
        validate_targets(root, targets)
        configuration = json.loads((root / "config/toolchain.json").read_text())
        cargo = ["cargo", "+" + configuration["fuzz_rust_toolchain"]]
        environment = fuzz_environment()
        version = subprocess.run(
            cargo + ["fuzz", "--version"], cwd=root, env=environment,
            text=True, capture_output=True, check=True, timeout=30,
        ).stdout.strip()
        if version != "cargo-fuzz " + configuration["cargo_fuzz_version"]:
            raise ValueError(f"unexpected cargo-fuzz version: {version}")
        identity = source_identity(root)
        result.update({
            "source_revision": subprocess.run(
                ["git", "rev-parse", "HEAD"], cwd=root, capture_output=True,
                text=True, check=True,
            ).stdout.strip(),
            "working_tree_dirty": bool(subprocess.run(
                ["git", "status", "--porcelain"], cwd=root, capture_output=True,
                text=True, check=True,
            ).stdout.strip()),
            "source_sha256": identity, "toolchain": configuration["fuzz_rust_toolchain"],
            "cargo_fuzz": version, "profile": arguments.profile,
            "limits": {"input_seconds": INPUT_TIMEOUT, "rss_mb": RSS_LIMIT_MB,
                       "max_input_bytes": MAX_INPUT_BYTES, "total_seconds": CAMPAIGN_SECONDS},
        })
        if arguments.mode == "run":
            if arguments.corpus_dir.is_symlink():
                raise ValueError("corpus directory must not be a symlink")
            corpus = arguments.corpus_dir.resolve()
            if corpus.exists() and (
                not corpus.is_dir() or {path.name for path in corpus.iterdir()} - set(TARGETS)
            ):
                raise ValueError("unsupported corpus collection")
            for target in TARGETS:
                if (corpus / target).exists() or (corpus / target).is_symlink():
                    inventory(corpus / target, MAX_INPUT_BYTES, MAX_CORPUS_FILES)
            seeded = bounded(
                cargo + ["run", "--quiet", "--example", "fuzz_seeds", "--", str(corpus)],
                output / "seeds.log", 60,
            )
            result["seeds"] = seeded
            if seeded["exit_code"] or seeded["timed_out"]:
                raise ValueError("generated fuzz seeds failed; see seeds.log")
            for target in targets:
                inventory(corpus / target, MAX_INPUT_BYTES, MAX_CORPUS_FILES)
            result["seed_provenance"] = "Generated project fixture and constant byte patterns; no official/private assets"
        else:
            if arguments.artifact.is_symlink():
                raise ValueError("replay input must not be a symlink")
            artifact = arguments.artifact.resolve()
            if not artifact.is_file() or artifact.stat().st_size > MAX_INPUT_BYTES:
                raise ValueError("replay input must be a bounded regular file")
            result["input_sha256"] = hashlib.sha256(artifact.read_bytes()).hexdigest()
        built = bounded(
            cargo + ["fuzz", "build"] + (targets if len(targets) == 1 else []),
            output / "build.log", 180,
        )
        result["build"] = built
        if built["exit_code"] or built["timed_out"]:
            raise ValueError("fuzz build failed; see build.log")
        for target in targets:
            artifacts = output / target
            artifacts.mkdir()
            seconds = PROFILES[arguments.profile]
            inputs = corpus / target if arguments.mode == "run" else artifact
            outcome = bounded(
                fuzz_command(cargo, target, inputs, artifacts, seconds, arguments.mode == "replay"),
                output / (target + ".log"), seconds + 10,
            )
            outcome["target"] = target
            outcome["artifacts"] = inventory(artifacts, MAX_INPUT_BYTES, MAX_CORPUS_FILES)
            if arguments.mode == "run":
                outcome["corpus"] = corpus_summary(
                    inventory(corpus / target, MAX_INPUT_BYTES, MAX_CORPUS_FILES)
                )
            outcome["status"] = "failed"
            result["targets"].append(outcome)
            if not outcome["exit_code"] and not outcome["timed_out"] and not outcome["artifacts"]:
                outcome["executions"] = execution_count(output / outcome["log"])
                if arguments.mode == "replay" and outcome["executions"] != 1:
                    raise ValueError(f"single-input replay requires exactly one execution: {target}")
                outcome["status"] = "passed"
        if source_identity(root) != identity:
            raise ValueError("fuzz source changed during the campaign")
        if time.monotonic() >= deadline:
            raise ValueError("fuzz campaign total deadline exhausted")
        if arguments.mode == "replay" and hashlib.sha256(artifact.read_bytes()).hexdigest() != result["input_sha256"]:
            raise ValueError("replay input changed during execution")
        if all(item["status"] == "passed" for item in result["targets"]):
            result["executions"] = sum(item["executions"] for item in result["targets"])
            result["status"] = "passed"
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        result["error"] = str(error)
        print(f"fuzz validation failed: {error}", file=sys.stderr)
    (output / "report.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("run", "replay"))
    parser.add_argument("--profile", choices=tuple(PROFILES), default="smoke")
    parser.add_argument("--targets", nargs="+", choices=TARGETS, default=list(TARGETS))
    parser.add_argument("--corpus-dir", type=Path, default=ROOT / "target/fuzz-corpus")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--artifact", type=Path)
    arguments = parser.parse_args()
    if arguments.mode == "replay" and (arguments.artifact is None or len(arguments.targets) != 1):
        parser.error("replay requires one target and --artifact")
    try:
        result = run(arguments)
    except (OSError, ValueError) as error:
        print(f"fuzz setup failed: {error}", file=sys.stderr)
        return 1
    summary = {"status": result["status"], "targets": len(result["targets"])}
    if "executions" in result:
        summary["executions"] = result["executions"]
    print(json.dumps(summary, indent=2))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
