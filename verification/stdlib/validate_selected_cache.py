"""Require fresh selected queries without changing source or rebuilding the interface."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
from proof_sensitivity import proof_workspace, selected_proof


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--verus", type=Path, required=True)
arguments = parser.parse_args()
results = []
with proof_workspace(ROOT, arguments.verus.resolve(), "jbig2-selected-cache-") as (
    directory, environment, expected, rust,
):
    source_files = {
        path: (path.read_bytes(), path.stat().st_mtime_ns)
        for name in ("src", "specs", "proofs")
        for path in (directory / name).rglob("*.rs")
    }
    dependencies = None
    for module, function, scope in (
        ("mmr", "observed_transition_bytes", None),
        ("mmr", "reference_elements", None),
        ("mmr", "observed_transition_bytes", None),
        ("symbol_text", ("ArithmeticCodec::decode_integer", "ArithmeticCodec::decode_symbol_id"),
         "symbol_text::codec"),
    ):
        result = selected_proof(
            arguments.verus.resolve(), directory, environment, False, module, function,
            verification_module=scope,
        )
        if type(result["verified"]) is not int or result["verified"] < 1 or result["errors"] != 0:
            raise ValueError(f"the unchanged-source query must freshly prove its target: {function}: {result}")
        current = {
            path.name: (hashlib.sha256(path.read_bytes()).hexdigest(), path.stat().st_mtime_ns)
            for path in (directory / "proof-target/verus-partial/debug/deps").glob(
                "libjbig2_capacity_interface-*"
            )
            if path.suffix in {".rmeta", ".vir"}
        }
        if len(current) != 2 or dependencies is not None and current != dependencies:
            raise ValueError(
                "only the production package should be invalidated, not the interface dependency: "
                + json.dumps({
                    "previous": dependencies, "current": current,
                    "artifact_locations": [
                        path.relative_to(directory).as_posix()
                        for path in directory.rglob("*jbig2_capacity_interface*")
                        if path.is_file()
                    ],
                })
            )
        dependencies = current
        results.append({
            "module": module, "function": function,
            "verification_module": scope or module, "verification": result,
        })
    if any((path.read_bytes(), path.stat().st_mtime_ns) != original
           for path, original in source_files.items()):
        raise ValueError("fresh selected queries must not require source edits or timestamp touches")
report = {
    "status": "passed", "source_modified": False,
    "interface_artifact_reused": True, "queries": results,
    "trust_decision_from_controls": False, "hosted_acceptance": False,
}
output = ROOT / "target/capacity-proposal/selected-cache-regression.json"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
