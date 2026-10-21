"""Source identities for named, ordinary Rust tests; not execution evidence."""

import re
import subprocess
import sys
from pathlib import Path

from rust_source import excluded, parse, source_functions


def discover_tests(root: Path, targets) -> set:
    targets = sorted(set(targets))
    if not targets:
        raise ValueError("Cargo discovery requires at least one test target")
    result = set()
    for target in targets:
        selector = ["--lib"] if target == "lib" else ["--test", target]
        process = subprocess.run(
            ["cargo", "test", "--locked", "--all-features", *selector, "--", "--list"],
            cwd=root, text=True, capture_output=True, check=False,
        )
        if process.returncode:
            print(process.stderr, file=sys.stderr)
            process.check_returncode()
        result.update((target, line.removesuffix(": test")) for line in process.stdout.splitlines()
                      if line.endswith(": test"))
    return result


def test_inventory(root: Path) -> dict:
    root = root.resolve()
    modules = {entry["file"]: entry["file_module"] for entry in source_functions(root)}
    result = {}

    def walk(container, target, prefix, path, inherited_ignore=False):
        if excluded(
            [node for node in container.named_children if node.type == "inner_attribute_item"],
            True, testing=True,
        ):
            return
        attributes = []
        for node in container.named_children:
            if node.type in {"line_comment", "block_comment"}:
                continue
            if node.type == "attribute_item":
                attributes.append(node)
                continue
            if not excluded(attributes, True, testing=True):
                ignored = inherited_ignore or any(
                    re.match(rb"#\[\s*ignore\b", attribute.text) for attribute in attributes
                )
                if node.type == "function_item" and any(
                    re.fullmatch(rb"#\[\s*test\s*\]", attribute.text) for attribute in attributes
                ):
                    name = node.child_by_field_name("name").text.decode("utf-8")
                    full = "::".join([*prefix, name])
                    key = (target, full)
                    if key in result:
                        raise ValueError(f"ambiguous test identity: {key}")
                    result[key] = {
                        "target": target, "name": full, "ignored": bool(ignored),
                        "file": path.relative_to(root).as_posix(),
                        "line": node.start_point.row + 1,
                    }
                elif node.type == "mod_item" and node.child_by_field_name("body") is not None:
                    name = node.child_by_field_name("name").text.decode("utf-8")
                    walk(node.child_by_field_name("body"), target, [*prefix, name], path, ignored)
            attributes = []

    for path in sorted((root / "src").rglob("*.rs")):
        relative = path.relative_to(root).as_posix()
        parts = path.relative_to(root / "src").with_suffix("").parts
        fallback = "::".join(parts[:-1] if path.name == "mod.rs" else parts)
        prefix = modules.get(relative, "" if path.name == "lib.rs" else fallback)
        walk(parse(path.read_bytes()), "lib", prefix.split("::") if prefix else [], path)
    for path in sorted((root / "tests").glob("*.rs")):
        walk(parse(path.read_bytes()), path.stem, [], path)
    return result
