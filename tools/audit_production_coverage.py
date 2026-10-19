#!/usr/bin/env python3
import argparse
import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FUNCTION = re.compile(
    r"(?m)^[ \t]*(?P<visibility>pub(?:\([^)]*\))?[ \t]+)?"
    r"(?:const[ \t]+)?fn[ \t]+(?P<name>[A-Za-z_][A-Za-z0-9_]*)"
)
BLOCK = re.compile(
    r"(?m)^[ \t]*(?:(?:#\[[^\n]+\]\s*)*)"
    r"(?P<kind>impl|trait)(?P<header>[^{]*)\{"
)


def code_without_strings(source: str) -> str:
    code = re.sub(
        r'"(?:\\.|[^"\\])*"',
        lambda match: re.sub(r"[^\r\n]", " ", match.group()),
        source,
    )
    code = re.sub(
        r"//[^\r\n]*",
        lambda match: " " * len(match.group()),
        code,
    )
    return re.sub(
        r"/\*.*?\*/",
        lambda match: re.sub(r"[^\r\n]", " ", match.group()),
        code,
        flags=re.DOTALL,
    )


def production_source(source: str) -> str:
    return re.split(
        r"#\[cfg\(test\)\]\s*mod\s+tests",
        source,
        maxsplit=1,
    )[0]


def item_attributes(source: str, position: int) -> list[str]:
    cursor = position
    attributes = []
    while True:
        while cursor > 0 and source[cursor - 1].isspace():
            cursor -= 1
        if cursor == 0 or source[cursor - 1] != "]":
            break
        end = cursor
        depth = 0
        cursor -= 1
        while cursor >= 0:
            depth += (source[cursor] == "]") - (source[cursor] == "[")
            if depth == 0:
                break
            cursor -= 1
        if cursor <= 0 or source[cursor - 1] != "#":
            break
        cursor -= 1
        attributes.append(source[cursor:end])
    return attributes


def is_nonproduction_item(source: str, position: int) -> bool:
    attributes = ["".join(attribute.split()) for attribute in item_attributes(source, position)]
    return any(
        attribute in {
            "#[cfg(test)]",
            "#[cfg(verus_keep_ghost)]",
            "#[cfg(any(test,verus_keep_ghost))]",
        }
        for attribute in attributes
    )


def nonproduction_types(source: str) -> set[str]:
    declaration = re.compile(r"\b(?:struct|enum)\s+([A-Za-z_]\w*)")
    return {
        match.group(1)
        for match in declaration.finditer(code_without_strings(source))
        if is_nonproduction_item(source, match.start())
    }


def block_owner(kind: str, header: str) -> str | None:
    header = re.sub(r"\s+", " ", header).strip()
    if kind == "trait":
        match = re.match(r"([A-Za-z_][A-Za-z0-9_]*)", header)
        return match.group(1) if match else None
    if header.startswith("<"):
        depth = 0
        for index, character in enumerate(header):
            depth += (character == "<") - (character == ">")
            if depth == 0:
                header = header[index + 1 :].strip()
                break
    target = header.split(" for ", maxsplit=1)[-1]
    target = target.split(" where ", maxsplit=1)[0].strip()
    target = re.sub(r"^&\s*(?:'\w+\s*)?(?:mut\s+)?", "", target)
    match = re.search(r"([A-Za-z_][A-Za-z0-9_]*)\s*(?:<.*>)?$", target)
    return match.group(1) if match else None


def blocks(source: str) -> list[dict]:
    code = code_without_strings(source)
    result = []
    for match in BLOCK.finditer(code):
        depth = 1
        cursor = match.end()
        while cursor < len(code) and depth:
            depth += (code[cursor] == "{") - (code[cursor] == "}")
            cursor += 1
        if depth:
            raise RuntimeError(f"unterminated {match['kind']} block")
        result.append(
            {
                "start": match.end(),
                "end": cursor,
                "kind": match["kind"],
                "owner": block_owner(match["kind"], match["header"]),
            }
        )
    return result


def included_modules() -> dict[str, str]:
    result = {}
    declaration = re.compile(
        r'#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]\s*'
        r"(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*;"
    )
    for parent in sorted((ROOT / "src").glob("*.rs")):
        source = production_source(parent.read_text(encoding="utf-8"))
        for match in declaration.finditer(source):
            included = (parent.parent / match.group(1)).resolve()
            if included.parent == (ROOT / "src").resolve():
                result[included.stem] = f"{parent.stem}::{match.group(2)}"
    return result


def type_modules(module_paths: dict[str, str]) -> dict[str, str]:
    result = {}
    declaration = re.compile(r"\b(?:struct|enum)\s+([A-Za-z_]\w*)")
    for path in sorted((ROOT / "src").glob("*.rs")):
        source = production_source(path.read_text(encoding="utf-8"))
        for match in declaration.finditer(code_without_strings(source)):
            result[match.group(1)] = module_paths.get(path.stem, path.stem)
    return result


def source_functions() -> list[dict]:
    result = []
    module_paths = included_modules()
    owners = type_modules(module_paths)
    for path in sorted((ROOT / "src").glob("*.rs")):
        if path.name == "lib.rs":
            continue
        source = production_source(path.read_text(encoding="utf-8"))
        try:
            scopes = blocks(source)
        except RuntimeError as error:
            raise RuntimeError(f"{path.relative_to(ROOT)}: {error}") from error
        excluded_owners = nonproduction_types(source)
        for match in FUNCTION.finditer(code_without_strings(source)):
            if is_nonproduction_item(source, match.start()):
                continue
            scope = next(
                (
                    block
                    for block in scopes
                    if block["start"] <= match.start() < block["end"]
                ),
                None,
            )
            owner = scope["owner"] if scope else None
            if owner in excluded_owners:
                continue
            identifier = f"{path.stem}::"
            if owner:
                identifier += f"{owner}::"
            identifier += match["name"]
            result.append(
                {
                    "id": identifier,
                    "file": path.relative_to(ROOT).as_posix(),
                    "visibility": (match["visibility"] or "private").strip(),
                    "owner": owner,
                    "verifier_module": owners.get(
                        owner, module_paths.get(path.stem, path.stem)
                    ),
                    "name": match["name"],
                    "file_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            )
    return result


def verified_matches(entry: dict, details: set[str]) -> list[str]:
    prefix = f"jbig2_rs_production::{entry['verifier_module']}::"
    candidates = [
        item
        for item in details
        if item.startswith(prefix) and item.endswith(f"::{entry['name']}")
    ]
    if entry["owner"]:
        owned = [
            item
            for item in candidates
            if f"::{entry['owner']}::" in item
        ]
        if owned:
            candidates = owned
    return sorted(candidates)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verus-json", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--dispositions",
        type=Path,
        default=ROOT / "proof-dispositions.json",
    )
    parser.add_argument("--require-complete", action="store_true")
    arguments = parser.parse_args()

    report = json.loads(arguments.verus_json.read_text(encoding="utf-8"))
    verification = report.get("verification-results", {})
    if not verification.get("success") or verification.get("errors") != 0:
        raise RuntimeError("production Verus report is not successful")
    details = set(report.get("func-details", {}))
    dispositions = {}
    if arguments.dispositions.exists():
        disposition_document = json.loads(
            arguments.dispositions.read_text(encoding="utf-8")
        )
        dispositions = {
            entry["id"]: entry for entry in disposition_document["functions"]
        }

    records = []
    identifiers: dict[str, int] = {}
    consumed_dispositions = set()
    for entry in source_functions():
        base_identifier = entry["id"]
        identifiers[base_identifier] = identifiers.get(base_identifier, 0) + 1
        if identifiers[base_identifier] > 1:
            entry["id"] = f"{base_identifier}#{identifiers[base_identifier]}"
        matches = verified_matches(entry, details)
        disposition = dispositions.get(entry["id"])
        if matches:
            status = "verified"
            reason = None
        elif disposition is not None:
            if disposition["file_sha256"] != entry["file_sha256"]:
                raise RuntimeError(
                    f"stale proof disposition for {entry['id']}"
                )
            status = disposition["status"]
            if status not in {
                "reviewed-verifier-limitation",
                "audited-forwarder",
                "native-assumption",
            }:
                raise RuntimeError(
                    f"invalid proof disposition status for {entry['id']}: {status}"
                )
            reason = disposition["reason"]
            consumed_dispositions.add(entry["id"])
        else:
            status = "incomplete"
            reason = None
        records.append(
            {
                "id": entry["id"],
                "file": entry["file"],
                "visibility": entry["visibility"],
                "status": status,
                "disposition_reason": reason,
                "file_sha256": entry["file_sha256"],
                "verus_functions": matches,
            }
        )

    unused = sorted(set(dispositions) - consumed_dispositions)
    if unused:
        raise RuntimeError(f"unused proof dispositions: {unused}")

    counts = {
        status: sum(record["status"] == status for record in records)
        for status in (
            "verified",
            "reviewed-verifier-limitation",
            "audited-forwarder",
            "native-assumption",
            "incomplete",
        )
    }
    output = {
        "schema_version": 1,
        "source": "src/*.rs",
        "verus_verified_obligations": verification["verified"],
        "native_assumptions": [],
        "counts": counts,
        "functions": records,
    }
    arguments.output.write_text(
        json.dumps(output, indent=2) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(counts, sort_keys=True))
    if arguments.require_complete and counts["incomplete"]:
        raise RuntimeError(
            f"{counts['incomplete']} production functions remain incomplete"
        )


if __name__ == "__main__":
    main()
