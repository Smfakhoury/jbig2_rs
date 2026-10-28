"""Bind edits to a real Rust module's source files after physical decomposition."""

import hashlib
from pathlib import Path, PurePosixPath

from rust_source import source_functions, source_modules


class SourceGroup:
    def __init__(self, root: Path, entry: str, modules=None):
        self.root = root.resolve()
        relative = PurePosixPath(entry)
        if relative.is_absolute() or ".." in relative.parts or "\\" in entry:
            raise ValueError(f"invalid source-group entry: {entry}")
        modules = (
            source_modules(self.root) if entry.startswith("src/") else {}
        ) if modules is None else modules
        prefix = modules.get(entry)
        names = {entry}
        if prefix:
            names.update(
                name for name, module in modules.items()
                if module == prefix or module.startswith(prefix + "::")
            )
        self.names = (entry, *sorted(names - {entry}))
        self.original = {}
        self.logical = {}
        for name in self.names:
            path = self.root / name
            if (
                not path.is_file() or path.is_symlink()
                or not path.resolve().is_relative_to(self.root)
                or any(parent.is_symlink() for parent in path.parents if parent.is_relative_to(self.root))
            ):
                raise ValueError(f"missing, external or symlinked grouped source: {name}")
            self.original[name] = path.read_bytes()
            self.logical[name] = path.read_text(encoding="utf-8")
        self.markers = tuple(
            "\n// jbig2-bound-source:" + hashlib.sha256(name.encode()).hexdigest() + "\n"
            for name in (*self.names, "<end>")
        )
        if any(marker in text for marker in self.markers for text in self.logical.values()):
            raise ValueError("source text collides with a group boundary")
        self.text = (
            self.logical[entry] if len(self.names) == 1
            else "".join(marker + self.logical[name] for marker, name in zip(self.markers, self.names))
            + self.markers[-1]
        )

    def contents(self, text):
        if len(self.names) == 1:
            return {self.names[0]: text}
        if any(text.count(marker) != 1 for marker in self.markers):
            raise ValueError("source-group boundaries were changed")
        if not text.startswith(self.markers[0]) or not text.endswith(self.markers[-1]):
            raise ValueError("source-group boundaries were reordered")
        pieces = {}
        remaining = text
        for index, name in enumerate(self.names):
            if not remaining.startswith(self.markers[index]):
                raise ValueError("source-group boundaries were reordered")
            value, remaining = remaining[len(self.markers[index]):].split(self.markers[index + 1], 1)
            pieces[name] = value
            remaining = self.markers[index + 1] + remaining
        if remaining != self.markers[-1]:
            raise ValueError("source-group boundary content changed")
        return pieces

    def write(self, directory: Path, text: str):
        pieces = self.contents(text)
        for name, value in pieces.items():
            path = directory / name
            if value == self.logical[name]:
                path.write_bytes(self.original[name])
            else:
                path.write_text(value, encoding="utf-8")

    def restore(self, directory: Path):
        for name, value in self.original.items():
            (directory / name).write_bytes(value)


def bound_source_groups(root, files):
    modules = source_modules(root) if any(name.startswith("src/") for name in files) else {}
    groups = {name: SourceGroup(root, name, modules) for name in files}
    sources = {}
    for group in groups.values():
        sources.update(group.original)
    return groups, sources, modules


def relocated_target(root, module, function, files, inventory=None):
    inventory = source_functions(root) if inventory is None else inventory
    matches = [
        row for row in inventory
        if row["file"] in files
        and (row["verifier_module"] == module or row["verifier_module"].startswith(module + "::"))
        and (row["owner"] + "::" if row["owner"] else "") + row["name"] == function
    ]
    if len(matches) > 1:
        raise ValueError(f"ambiguous relocated proof target: {module}::{function}")
    # Ghost lemmas are absent from ordinary Rust inventory; their explicitly
    # configured identity is still required by selected_proof's verifier report.
    return (matches[0]["verifier_module"], function) if matches else (module, function)
