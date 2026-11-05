#!/usr/bin/env python3
"""Export an isolated, pinned native compiler inventory without changing source."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

from check_distribution import isolated_environment
from compiler_inventory import reconcile
from evidence import load_json


ROOT = Path(__file__).resolve().parents[1]
PACKAGE_FILES = ("Cargo.toml", "Cargo.lock", "README.md", "SECURITY.md", "LICENSE")
TOOL_FILES = (
    "tools/collect_compiler_inventory.py", "tools/compiler_inventory.py",
    "tools/check_distribution.py", "tools/rust_source.py", "tools/check_traceability.py",
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bindings(root):
    paths = [root / name for name in (*PACKAGE_FILES, *TOOL_FILES, "config/toolchain.json")]
    paths.extend(sorted((root / "src").rglob("*.rs")))
    if any(path.is_symlink() for path in paths):
        raise ValueError("compiler inventory inputs must not be symlinks")
    return {path.relative_to(root).as_posix(): digest(path) for path in paths}


def execute(command, directory, environment, output, label):
    process = subprocess.run(
        command, cwd=directory, env=environment, capture_output=True, text=True, check=False,
    )
    (output / (label + ".stdout")).write_text(process.stdout)
    (output / (label + ".stderr")).write_text(process.stderr)
    if process.returncode:
        raise ValueError(
            f"{label} exited {process.returncode}; see {label}.stdout/.stderr: "
            + process.stderr[-2000:]
        )
    return process.stdout


def native_library(messages, directory):
    directory = directory.resolve()
    libraries = []
    for line in messages.splitlines():
        message = json.loads(line)
        if message["reason"] != "compiler-artifact":
            continue
        target = message["target"]
        if target["kind"] != ["lib"] or target["name"] != "jbig2_decoder":
            raise ValueError("unexpected compiled inventory target")
        if (
            Path(message["manifest_path"]).resolve() != directory / "Cargo.toml"
            or Path(target["src_path"]).resolve() != directory / "src/lib.rs"
            or message["profile"]["test"] is not False
        ):
            raise ValueError("inventory must compile the isolated ordinary production library")
        for filename in message["filenames"]:
            path = Path(filename).resolve()
            if path.suffix == ".rlib":
                if not path.is_relative_to(directory / "target") or path.stat().st_size == 0:
                    raise ValueError("missing, empty or out-of-tree native compiler library")
                libraries.append(path)
    if len(libraries) != 1:
        raise ValueError("compiler inventory needs exactly one actual native library")
    return libraries[0]


def commands(toolchain):
    cargo = ["cargo", "+" + toolchain]
    return [
        cargo + ["metadata", "--locked", "--offline", "--format-version=1"],
        cargo + [
            "rustdoc", "--locked", "--offline", "--lib", "--", "-Z", "unstable-options",
            "--output-format", "json", "--document-private-items", "--document-hidden-items",
        ],
        cargo + [
            "rustc", "--locked", "--offline", "--lib", "--message-format=json", "--",
            "--emit=mir,link", "-Zmir-include-spans=yes", "-Ztrim-diagnostic-paths=no",
        ],
    ]


def validate_existing(root, output):
    report = load_json(output / "report.json")
    if report["source_bindings"] != bindings(root):
        raise ValueError("stale compiler inventory source/tool bindings")
    expected_artifacts = {"compiler-items.json", "compiler-mir.txt", "native-library.rlib"}
    if set(report["artifacts"]) != expected_artifacts:
        raise ValueError("missing or invented compiler inventory artifacts")
    for name in expected_artifacts:
        path = output / name
        if path.is_symlink() or report["artifacts"][name] != {
            "sha256": digest(path), "bytes": path.stat().st_size,
        } or path.stat().st_size == 0:
            raise ValueError("stale, modified or empty original compiler artifact")
    compiler = load_json(root / "config/toolchain.json")["coverage"]["rust_toolchain"]
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    profile = report["profile"]
    if (
        profile["toolchain_source"] != "config/toolchain.json coverage.rust_toolchain"
        or profile["toolchain"] != compiler or profile["commands"] != commands(compiler)
        or profile["package"] != package["name"] or profile["version"] != package["version"]
        or profile["production_library"] is not True or profile["tests_enabled"] is not False
        or profile["verus_keep_ghost"] is not False or profile["dependencies"] != []
        or profile["features"] != [] or profile["isolated_cargo_home"] is not True
        or profile["rustc_identity"] != (output / "rustc-version.stdout").read_text()
        or profile["rustdoc_identity"] != (output / "rustdoc-version.stdout").read_text()
    ):
        raise ValueError("compiler inventory has the wrong isolated build profile")
    document = load_json(output / "compiler-items.json")
    if document["crate_version"] != package["version"]:
        raise ValueError("compiler metadata has the wrong package version")
    recomputed = reconcile(
        root, document,
        (output / "compiler-mir.txt").read_text(),
    )
    if any(report.get(key) != value for key, value in recomputed.items()):
        raise ValueError("compiler inventory claims differ from actual source/item/MIR reconstruction")
    return report


def collect(root, output):
    source_bindings = bindings(root)
    config = load_json(root / "config/toolchain.json")
    toolchain = config["coverage"]["rust_toolchain"]
    if not toolchain.startswith("nightly-"):
        raise ValueError("private compiler metadata requires the existing pinned nightly profile")
    with tempfile.TemporaryDirectory(prefix="jbig2-compiler-inventory-") as temporary:
        directory = Path(temporary).resolve()
        if directory.is_relative_to(root):
            raise ValueError("compiler export must execute outside the checkout")
        shutil.copytree(root / "src", directory / "src")
        for name in PACKAGE_FILES:
            shutil.copyfile(root / name, directory / name)
        for name, expected in source_bindings.items():
            if name in PACKAGE_FILES or name.startswith("src/"):
                if digest(directory / name) != expected:
                    raise ValueError("isolated native compiler source differs from the frozen input")
        environment = isolated_environment(directory)
        environment.update({"CARGO_BUILD_JOBS": "1", "CARGO_INCREMENTAL": "0"})
        rustc = execute(
            ["rustc", "+" + toolchain, "--version", "--verbose"],
            directory, environment, output, "rustc-version",
        )
        rustdoc = execute(
            ["rustdoc", "+" + toolchain, "--version", "--verbose"],
            directory, environment, output, "rustdoc-version",
        )
        metadata_command, doc_command, mir_command = commands(toolchain)
        metadata = json.loads(execute(
            metadata_command, directory, environment, output, "cargo-metadata",
        ))
        packages = metadata["packages"]
        if (
            len(packages) != 1 or packages[0]["name"] != "jbig2-decoder"
            or packages[0]["dependencies"] or packages[0]["features"]
            or Path(packages[0]["manifest_path"]).resolve() != directory / "Cargo.toml"
        ):
            raise ValueError("compiler inventory must not introduce dependencies or feature narrowing")
        execute(doc_command, directory, environment, output, "compiler-items")
        messages = execute(mir_command, directory, environment, output, "compiler-mir")
        library = native_library(messages, directory)
        mir_paths = list((directory / "target").rglob("*.mir"))
        if len(mir_paths) != 1:
            raise ValueError("missing or ambiguous fresh compiler MIR export")
        doc_path = directory / "target/doc/jbig2_decoder.json"
        document = load_json(doc_path)
        manifest = tomllib.loads((directory / "Cargo.toml").read_text())
        if document["crate_version"] != manifest["package"]["version"]:
            raise ValueError("compiler metadata has the wrong package version")
        result = reconcile(root, document, mir_paths[0].read_text())
        for source, name in (
            (doc_path, "compiler-items.json"), (mir_paths[0], "compiler-mir.txt"),
            (library, "native-library.rlib"),
        ):
            shutil.copyfile(source, output / name)
        result.update(
            source_bindings=source_bindings,
            profile={
                "toolchain_source": "config/toolchain.json coverage.rust_toolchain",
                "toolchain": toolchain, "rustc_identity": rustc, "rustdoc_identity": rustdoc,
                "package": packages[0]["name"], "version": packages[0]["version"],
                "production_library": True, "tests_enabled": False, "verus_keep_ghost": False,
                "dependencies": [], "features": [], "isolated_cargo_home": True,
                "commands": [metadata_command, doc_command, mir_command],
            },
            artifacts={
                name: {"sha256": digest(output / name), "bytes": (output / name).stat().st_size}
                for name in ("compiler-items.json", "compiler-mir.txt", "native-library.rlib")
            },
        )
    if bindings(root) != source_bindings:
        raise ValueError("source or inventory tooling changed during compiler export")
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--check-existing", action="store_true")
    arguments = parser.parse_args()
    output = arguments.output_dir.resolve()
    target = ROOT / "target"
    if not output.is_relative_to(target.resolve()) or output == target.resolve():
        parser.error("compiler inventory output must be a named directory inside target")
    if not arguments.check_existing:
        output.mkdir(parents=True, exist_ok=True)
    try:
        if arguments.check_existing:
            result = validate_existing(ROOT, output)
        else:
            result = collect(ROOT, output)
            (output / "report.json").write_text(json.dumps(result, indent=2) + "\n")
            validate_existing(ROOT, output)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        print(f"compiler inventory failed: {error}", file=sys.stderr)
        result = {"schema_version": 1, "status": "failed", "error": str(error)}
    if not arguments.check_existing:
        (output / "report.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({key: value for key, value in result.items() if key != "records"
                     and key != "source_bindings" and key != "public_root_exports"}, indent=2))
    return 0 if result["status"] != "failed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
