#!/usr/bin/env python3
"""Build and exercise the actual Cargo archive outside the repository."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from pathlib import Path

from audit_package import REQUIRED, audit


ROOT = Path(__file__).resolve().parents[1]


def isolated_environment(directory: Path) -> dict[str, str]:
    environment = {
        key: value for key, value in os.environ.items()
        if key not in {
            "RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_ENCODED_RUSTDOCFLAGS", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTFLAGS", "RUSTUP_TOOLCHAIN",
            "CARGO_BUILD_TARGET", "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC",
            "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        } and not (key.startswith("CARGO_TARGET_") and key.endswith("_RUSTFLAGS"))
    }
    environment.update({
        "CARGO_HOME": str(directory / "cargo-home"),
        "CARGO_TARGET_DIR": str(directory / "target"),
        "CARGO_NET_OFFLINE": "true",
        "CARGO_TERM_COLOR": "never",
    })
    return environment


def run(command: list[str], directory: Path, environment: dict, capture=False):
    result = subprocess.run(
        command, cwd=directory, env=environment, text=True,
        capture_output=capture, check=False,
    )
    if result.returncode:
        if capture:
            print(result.stdout, file=sys.stderr)
            print(result.stderr, file=sys.stderr)
        result.check_returncode()
    return result.stdout if capture else None


def extract_archive(archive: Path, directory: Path, package_root: str) -> list[str]:
    with tarfile.open(archive, "r:gz") as source:
        members = source.getmembers()
        failures = audit([member.name for member in members])
        if failures:
            raise ValueError("archive inventory failed: " + "; ".join(failures))
        for member in members:
            if not member.isfile() or not member.name.startswith(package_root + "/"):
                raise ValueError(f"invalid archive member: {member.name}")
        for member in members:
            target = directory.joinpath(*member.name.split("/"))
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(member) as contents:
                target.write_bytes(contents.read())
        return [member.name for member in members]


def metadata(cargo: list[str], directory: Path, environment: dict) -> dict:
    return json.loads(run(
        cargo + ["metadata", "--offline", "--locked", "--format-version=1"],
        directory, environment, capture=True,
    ))


def check_package_metadata(document: dict, extracted: Path, msrv: str) -> None:
    packages = document["packages"]
    if len(packages) != 1:
        raise ValueError("packaged decoder must have no runtime or development dependencies")
    package = packages[0]
    if (
        package["name"] != "jbig2_rs"
        or package["license"] != "MIT"
        or package["rust_version"] != msrv
        or package["dependencies"]
        or package["features"]
        or package["publish"] != []
        or Path(package["manifest_path"]).resolve() != (extracted / "Cargo.toml").resolve()
    ):
        raise ValueError("unexpected isolated package metadata or proof/validation features")


def check_distribution(toolchain: str) -> dict:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    if manifest.get("publish") is not False:
        raise ValueError("distribution readiness checks must not enable publication")
    package_root = f"{manifest['name']}-{manifest['version']}"
    cargo = ["cargo", f"+{toolchain}"]
    with tempfile.TemporaryDirectory(prefix="jbig2-distribution-") as temporary:
        directory = Path(temporary).resolve()
        if directory.is_relative_to(ROOT):
            raise ValueError("distribution validation must run outside the source checkout")
        environment = isolated_environment(directory)
        run(cargo + ["package", "--locked", "--offline", "--allow-dirty", "--no-verify"],
            ROOT, environment)
        archive = directory / "target" / "package" / f"{package_root}.crate"
        entries = extract_archive(archive, directory / "extracted", package_root)
        extracted = directory / "extracted" / package_root
        for path in REQUIRED - {"Cargo.toml"}:
            if (extracted / path).read_bytes() != (ROOT / path).read_bytes():
                raise ValueError(f"archive source differs from checkout: {path}")
        check_package_metadata(metadata(cargo, extracted, environment), extracted,
                               manifest["rust-version"])
        if any((extracted / name).exists() for name in ("proofs", "specs", "validation", "fuzz")):
            raise ValueError("proof or validation assets leaked into the extracted package")
        run(cargo + ["test", "--offline", "--locked", "--all-targets"], extracted, environment)
        run(cargo + ["test", "--offline", "--locked", "--doc"], extracted, environment)

        consumer = directory / "consumer"
        (consumer / "src").mkdir(parents=True)
        (consumer / "Cargo.toml").write_text(
            '[package]\nname = "distribution-consumer"\nversion = "0.0.0"\nedition = "2021"\n'
            '[workspace]\n[dependencies]\njbig2_rs = { path = '
            + json.dumps(extracted.as_posix(), ensure_ascii=False) + ' }\n',
            encoding="utf-8",
        )
        main = consumer / "src" / "main.rs"
        main.write_bytes((ROOT / "tools" / "distribution_consumer.rs").read_bytes())
        run(cargo + ["generate-lockfile", "--offline"], consumer, environment)
        graph = metadata(cargo, consumer, environment)
        if {package["name"] for package in graph["packages"]} != {"jbig2_rs", "distribution-consumer"}:
            raise ValueError("unexpected downstream dependencies")
        run(cargo + ["run", "--offline", "--locked"], consumer, environment)

        main.write_text("use jbig2_rs::runtime_access;\nfn main() {}\n", encoding="utf-8")
        negative = subprocess.run(
            cargo + ["check", "--offline", "--locked", "--message-format=json"],
            cwd=consumer, env=environment, text=True, capture_output=True, check=False,
        )
        diagnostics = [
            json.loads(line) for line in negative.stdout.splitlines() if line.strip()
        ]
        if negative.returncode == 0 or not any(
            entry.get("reason") == "compiler-message"
            and (entry["message"].get("code") or {}).get("code") == "E0432"
            and "runtime_access" in entry["message"]["message"]
            for entry in diagnostics
        ):
            raise ValueError(
                "normal downstream validation API visibility check did not fail as expected:\n"
                + negative.stdout + negative.stderr
            )
        return {
            "schema_version": 1, "status": "passed",
            "toolchain": toolchain,
            "rustc": run(["rustc", f"+{toolchain}", "--version"], directory, environment, True).strip(),
            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "package_files": len(entries),
            "package_tests": "passed", "package_doc_tests": "passed",
            "downstream_exact_output": "passed", "runtime_access": "not_exported",
            "normal_dependencies": [], "cargo_home": "fresh", "cargo_network": "offline",
            "separate_proof_assets": "absent",
            "proof_annotations": "cfg_inactive_source",
            "publication_enabled": False,
        }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--toolchain", default="1.89.0")
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    try:
        result = check_distribution(arguments.toolchain)
        text = json.dumps(result, indent=2) + "\n"
        if arguments.output:
            arguments.output.parent.mkdir(parents=True, exist_ok=True)
            arguments.output.write_text(text, encoding="utf-8")
        print(text, end="")
        return 0
    except (OSError, ValueError, tarfile.TarError, subprocess.CalledProcessError) as error:
        print(f"distribution validation failed: {error}", file=sys.stderr)
        if arguments.output:
            arguments.output.parent.mkdir(parents=True, exist_ok=True)
            arguments.output.write_text(json.dumps({
                "schema_version": 1, "status": "failed",
                "toolchain": arguments.toolchain, "error": str(error),
            }, indent=2) + "\n", encoding="utf-8")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
