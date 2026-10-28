"""Source identities for named, ordinary Rust tests; not execution evidence."""

import re
import subprocess
import sys
from pathlib import Path

from rust_source import excluded, parse, resolve_module_file


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
    result = {}
    active = set()

    def visit_file(path, target, prefix, inherited_ignore=False):
        if path.is_symlink():
            raise ValueError(f"symlinked test module: {path}")
        path = path.resolve()
        if path in active:
            raise ValueError(f"cyclic test module inclusion: {path}")
        if not path.is_relative_to(root):
            raise ValueError(f"test module outside repository: {path}")
        active.add(path)
        try:
            try:
                container = parse(path.read_bytes())
            except ValueError as error:
                raise ValueError(f"invalid test module {path}: {error}") from error
            walk(container, target, prefix, path, prefix, inherited_ignore)
        finally:
            active.remove(path)

    def walk(container, target, prefix, path, file_prefix, inherited_ignore=False):
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
                elif node.type == "mod_item":
                    name = node.child_by_field_name("name").text.decode("utf-8")
                    nested = [*prefix, name]
                    body = node.child_by_field_name("body")
                    if body is not None:
                        walk(body, target, nested, path, file_prefix, ignored)
                    else:
                        source = resolve_module_file(
                            root, path, "::".join(file_prefix), "::".join(prefix), name, attributes,
                        )
                        visit_file(source, target, nested, ignored)
            attributes = []

    visit_file(root / "src/lib.rs", "lib", [])
    for path in sorted((root / "tests").glob("*.rs")):
        visit_file(path, path.stem, [])
    return result
