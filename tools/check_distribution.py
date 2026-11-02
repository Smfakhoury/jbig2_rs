#!/usr/bin/env python3
"""Build and exercise the actual Cargo archive outside the repository."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from pathlib import Path

from audit_package import LIBRARY_NAME, PACKAGE_NAME, REQUIRED, audit


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
        and not (key.startswith("CARGO_REGISTRY") or key.startswith("CARGO_REGISTRIES_"))
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
        package["name"] != PACKAGE_NAME
        or package["license"] != "MIT"
        or package["rust_version"] != msrv
        or package["dependencies"]
        or package["features"]
        or package["publish"] != []
        or Path(package["manifest_path"]).resolve() != (extracted / "Cargo.toml").resolve()
    ):
        raise ValueError("unexpected isolated package metadata or proof/validation features")
    targets = package.get("targets")
    if not isinstance(targets, list) or any(not isinstance(target, dict) for target in targets):
        raise ValueError("packaged decoder must provide Cargo library target metadata")
    libraries = [target for target in targets if target.get("kind") == ["lib"]]
    if len(libraries) != 1 or libraries[0]["name"] != LIBRARY_NAME:
        raise ValueError("packaged Rust library must expose the public jbig2_decoder import")


def require_rejected_check(
    cargo: list[str], directory: Path, environment: dict, code: str, message: str,
) -> None:
    negative = subprocess.run(
        cargo + ["check", "--offline", "--locked", "--message-format=json"],
        cwd=directory, env=environment, text=True, capture_output=True, check=False,
    )
    diagnostics = [
        json.loads(line) for line in negative.stdout.splitlines() if line.strip()
    ]
    if negative.returncode == 0 or not any(
        entry.get("reason") == "compiler-message"
        and (entry["message"].get("code") or {}).get("code") == code
        and message in entry["message"]["message"]
        for entry in diagnostics
    ):
        raise ValueError(
            f"expected compilation rejection {code} for {message!r}:\n"
            + negative.stdout + negative.stderr
        )


def check_unsafe_policy(cargo: list[str], extracted: Path, environment: dict) -> None:
    library = extracted / "src" / "lib.rs"
    original = library.read_bytes()
    guard = b"#![forbid(unsafe_code)]"
    if original.count(guard) != 1:
        raise ValueError("packaged crate must have one explicit unsafe-code prohibition")
    probe = b"\n#[allow(dead_code, unused_unsafe)]\nfn unsafe_policy_probe() { unsafe {} }\n"
    try:
        library.write_bytes(original + probe)
        require_rejected_check(
            cargo, extracted, environment, "unsafe_code", "usage of an `unsafe` block",
        )
        library.write_bytes(original.replace(guard, b"#![allow(unsafe_code)]", 1) + probe)
        run(cargo + ["check", "--offline", "--locked"], extracted, environment, capture=True)
    finally:
        library.write_bytes(original)


def check_publish_dry_run(cargo: list[str], extracted: Path, environment: dict) -> dict:
    if extracted.resolve().is_relative_to(ROOT.resolve()):
        raise ValueError("publication rehearsal must run outside the source checkout")
    manifest = extracted / "Cargo.toml"
    original = manifest.read_bytes()
    before = tomllib.loads(original.decode("utf-8"))
    if before["package"].get("publish") is not False:
        raise ValueError("publication rehearsal requires the disabled original manifest")
    changed, count = re.subn(
        rb"(?m)^publish = false\r?$", b'publish = ["crates-io"]', original,
    )
    expected = tomllib.loads(original.decode("utf-8"))
    expected["package"]["publish"] = ["crates-io"]
    if count != 1 or tomllib.loads(changed.decode("utf-8")) != expected:
        raise ValueError("publication rehearsal must change only the package publish setting")
    command = cargo + ["publish", "--dry-run", "--locked", "--registry", "crates-io"]
    publication_environment = {**environment, "CARGO_NET_OFFLINE": "false"}
    try:
        manifest.write_bytes(changed)
        process = subprocess.run(
            command, cwd=extracted, env=publication_environment,
            text=True, capture_output=True, check=False, timeout=300,
        )
        output = process.stdout + process.stderr
        if process.returncode:
            print(output, file=sys.stderr)
            process.check_returncode()
        if "aborting upload due to dry run" not in output:
            raise ValueError("Cargo did not confirm the dry-run upload was aborted")
        package_root = f"{before['package']['name']}-{before['package']['version']}"
        archive = Path(environment["CARGO_TARGET_DIR"]) / "package" / f"{package_root}.crate"
        with tempfile.TemporaryDirectory(prefix="publish-verification-") as directory:
            target = Path(directory)
            entries = extract_archive(archive, target, package_root)
            for path in REQUIRED - {"Cargo.toml"}:
                if (target / package_root / path).read_bytes() != (extracted / path).read_bytes():
                    raise ValueError(f"dry-run archive source differs: {path}")
            staged = tomllib.loads((target / package_root / "Cargo.toml").read_text())
            if staged != expected:
                raise ValueError("dry-run archive manifest differs from the intended publish-only change")
        return {
            "status": "passed", "command": command, "exit_code": process.returncode,
            "registry": "crates-io", "network": "registry_metadata", "uploaded": False,
            "manifest_change": 'package.publish: false -> ["crates-io"]; temporary extraction only',
            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "package_files": len(entries),
            "source_bytes_match_distribution": True,
            "manifest_restored": True,
            "source_manifest_sha256": hashlib.sha256(original).hexdigest(),
            "rehearsal_manifest_sha256": hashlib.sha256(changed).hexdigest(),
            "cargo_output": output,
        }
    finally:
        manifest.write_bytes(original)


def check_distribution(toolchain: str, publish_dry_run: bool = False) -> dict:
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
        archive_sha256 = hashlib.sha256(archive.read_bytes()).hexdigest()
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
        check_unsafe_policy(cargo, extracted, environment)

        consumer = directory / "consumer"
        (consumer / "src").mkdir(parents=True)
        (consumer / "Cargo.toml").write_text(
            '[package]\nname = "distribution-consumer"\nversion = "0.0.0"\nedition = "2021"\n'
            '[workspace]\n[dependencies]\njbig2-decoder = { path = '
            + json.dumps(extracted.as_posix(), ensure_ascii=False) + ' }\n',
            encoding="utf-8",
        )
        main = consumer / "src" / "main.rs"
        main.write_bytes((ROOT / "tools" / "distribution_consumer.rs").read_bytes())
        run(cargo + ["generate-lockfile", "--offline"], consumer, environment)
        graph = metadata(cargo, consumer, environment)
        if {package["name"] for package in graph["packages"]} != {PACKAGE_NAME, "distribution-consumer"}:
            raise ValueError("unexpected downstream dependencies")
        run(cargo + ["run", "--offline", "--locked"], consumer, environment)

        main.write_text("use jbig2_decoder::runtime_access;\nfn main() {}\n", encoding="utf-8")
        require_rejected_check(cargo, consumer, environment, "E0432", "runtime_access")
        publication = check_publish_dry_run(cargo, extracted, environment) if publish_dry_run else None
        result = {
            "schema_version": 1, "status": "passed",
            "package_name": PACKAGE_NAME, "library_name": LIBRARY_NAME,
            "toolchain": toolchain,
            "rustc": run(["rustc", f"+{toolchain}", "--version"], directory, environment, True).strip(),
            "archive_sha256": archive_sha256,
            "package_files": len(entries),
            "package_tests": "passed", "package_doc_tests": "passed",
            "downstream_exact_output": "passed", "runtime_access": "not_exported",
            "normal_dependencies": [], "cargo_home": "fresh", "cargo_network": "offline",
            "separate_proof_assets": "absent",
            "proof_annotations": "cfg_inactive_source",
            "unsafe_policy": "compiler_rejected_probe_and_accepted_weaker_control",
            "publication_enabled": False,
        }
        if publication is not None:
            result["publish_dry_run"] = publication
        return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--toolchain", default="1.88.0")
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--publish-dry-run", action="store_true",
        help="Also rehearse Cargo publication in the temporary extracted package, without uploading.",
    )
    arguments = parser.parse_args()
    try:
        result = check_distribution(arguments.toolchain, arguments.publish_dry_run)
        text = json.dumps(result, indent=2) + "\n"
        if arguments.output:
            arguments.output.parent.mkdir(parents=True, exist_ok=True)
            arguments.output.write_text(text, encoding="utf-8")
        print(text, end="")
        return 0
    except (OSError, ValueError, tarfile.TarError, subprocess.CalledProcessError,
            subprocess.TimeoutExpired) as error:
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
