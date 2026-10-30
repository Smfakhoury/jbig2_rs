"""Exercise the reviewed getter's checked boundary without granting release credit."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--verus", type=Path, required=True)
arguments = parser.parse_args()
pin = json.loads((ROOT / "config/toolchain.json").read_text())["verus"]
SOURCE = ROOT / "verification/stdlib/vec-capacity-interface"
OUTPUT = ROOT / "target/capacity-proposal/interface-controls"
OUTPUT.mkdir(parents=True, exist_ok=True)
EXECUTABLE = arguments.verus.resolve().parent / "cargo-verus"
ENVIRONMENT = {
    **os.environ,
    "RUSTUP_TOOLCHAIN": pin["rust_toolchain"],
    "VERUS_PATH": str(arguments.verus.resolve()),
    "CARGO_BUILD_JOBS": "1",
}
version = subprocess.run(
    [str(arguments.verus.resolve()), "--version"],
    cwd=ROOT, env=ENVIRONMENT, check=True, capture_output=True, text=True,
)
if f"Version: {pin['version']}" not in version.stdout:
    raise ValueError("capacity interface controls require the pinned Verus version")


def reports(text):
    decoder = json.JSONDecoder()
    values = []
    while text.strip():
        value, end = decoder.raw_decode(text.lstrip())
        text = text.lstrip()[end:]
        values.append(value)
    return [
        value for value in values
        if "jbig2_capacity_interface::observe_capacity_budget" in value.get("func-details", {})
    ]


results = []
with tempfile.TemporaryDirectory(prefix="jbig2-capacity-interface-control-") as temporary:
    directory = Path(temporary)
    shutil.copyfile(SOURCE / "Cargo.toml", directory / "Cargo.toml")
    shutil.copyfile(SOURCE / "lib.rs", directory / "lib.rs")
    subprocess.run(
        ["cargo", f"+{pin['rust_toolchain']}", "generate-lockfile", "--offline"],
        cwd=directory, env=ENVIRONMENT, check=True, capture_output=True, text=True,
    )
    def registry_packages(lockfile):
        return {
            (package["name"], package["version"], package["source"], package["checksum"])
            for package in tomllib.loads(lockfile.read_text())["package"]
            if package.get("source", "").startswith("registry+")
        }

    if registry_packages(directory / "Cargo.lock") != registry_packages(ROOT / "verification/Cargo.lock"):
        raise ValueError("isolated interface controls must preserve every registry pin and checksum")
    original = (directory / "lib.rs").read_text()
    branch = """if bytes > limit {
        None
    } else {
        Some((observed, bytes))
    }"""
    if original.count(branch) != 1:
        raise ValueError("interface limit-check control site is missing or ambiguous")
    mutant = original.replace(branch, "Some((observed, bytes))", 1)
    weaker = mutant.replace("                && bytes <= limit,", ",", 1)
    if weaker == mutant:
        raise ValueError("interface weaker control did not remove the limit postcondition")
    for identity, source, expected_errors in (
        ("baseline", original, 0), ("missing-limit", mutant, 1),
        ("weaker-limit", weaker, 0), ("restored", original, 0),
    ):
        (directory / "lib.rs").write_text(source)
        process = subprocess.run(
            [str(EXECUTABLE), "build", "--manifest-path", str(directory / "Cargo.toml"),
             "--offline", "--locked", "--target-dir", str(directory / "target"),
             "--", "--num-threads", "1", "--rlimit", "30",
             "--triggers-mode", "silent", "--output-json"],
            cwd=directory, env=ENVIRONMENT, capture_output=True, text=True, timeout=300,
        )
        (OUTPUT / (identity + ".jsons")).write_text(process.stdout)
        (OUTPUT / (identity + ".log")).write_text(process.stderr)
        found = reports(process.stdout)
        if len(found) != 1:
            raise ValueError(f"{identity}: expected exactly one interface report\n{process.stderr}")
        if any(found[0]["verus"].get(key) != pin[key] for key in ("version", "commit")):
            raise ValueError(f"{identity}: interface report does not match the pinned verifier")
        result = found[0]["verification-results"]
        if (
            result["is-verifying-entire-crate"] is not True
            or type(result["errors"]) is not int or result["errors"] != expected_errors
            or type(result["verified"]) is not int or result["verified"] != (0 if expected_errors else 1)
            or (process.returncode == 0) is not (expected_errors == 0)
            or expected_errors and "postcondition not satisfied" not in process.stderr
        ):
            raise ValueError(f"{identity}: unexpected interface proof outcome\n{process.stderr}")
        results.append({"id": identity, "verified": result["verified"], "errors": result["errors"]})
if (SOURCE / "lib.rs").read_text() != original:
    raise ValueError("interface source changed while running isolated controls")
report = {
    "status": "passed", "trust_decision_from_controls": False, "normative_approval": False,
    "scope": "Actual checked callability/numeric-budget boundary; not standard semantic approval",
    "cases": results,
}
(OUTPUT / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
