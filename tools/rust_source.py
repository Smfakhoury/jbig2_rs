"""Syntax-aware source selection for audit tooling, not a Rust compiler."""

import re
from copy import deepcopy
from functools import lru_cache
from pathlib import Path

from tree_sitter import Language, Parser
import tree_sitter_rust


LANGUAGE = Language(tree_sitter_rust.language())


def parse(source: bytes):
    tree = Parser(LANGUAGE).parse(source)
    if tree.root_node.has_error:
        raise ValueError("Rust syntax could not be parsed; audit cannot proceed")
    return tree.root_node


def split_arguments(text: str) -> list[str]:
    parts = []
    start = depth = 0
    for index, char in enumerate(text):
        depth += (char == "(") - (char == ")")
        if char == "," and depth == 0:
            parts.append(text[start:index])
            start = index + 1
    if text[start:].strip():
        parts.append(text[start:])
    return parts


def cfg_value(expression: str, exclude_proofs: bool, testing=False) -> bool | None:
    expression = expression.strip()
    if expression == "test":
        return testing
    if expression in {"fuzzing", "jbig2_validation"}:
        return False
    if expression == "verus_keep_ghost":
        return False if exclude_proofs else None
    match = re.fullmatch(r"(all|any|not)\s*\((.*)\)", expression, flags=re.DOTALL)
    if not match:
        return None
    values = [cfg_value(part, exclude_proofs, testing) for part in split_arguments(match[2])]
    if match[1] == "not":
        if len(values) != 1:
            raise ValueError("invalid not() cfg")
        return None if values[0] is None else not values[0]
    if match[1] == "all":
        return False if False in values else (None if None in values else True)
    return True if True in values else (None if None in values else False)


def excluded(attributes, exclude_proofs: bool, testing=False) -> bool:
    result = False
    for attribute in attributes:
        text = attribute.text.decode("utf-8")
        match = re.fullmatch(r"#!?\[\s*cfg\s*\((.*)\)\s*\]", text, flags=re.DOTALL)
        if match:
            result |= cfg_value(match[1], exclude_proofs, testing) is False
        elif re.search(r"\bcfg_attr\s*\(.*\bcfg\s*\(", text, flags=re.DOTALL):
            raise ValueError("conditional cfg attributes need explicit audit support")
    return result


def items(container, exclude_proofs=True, testing=False):
    if excluded(
        [node for node in container.named_children if node.type == "inner_attribute_item"],
        exclude_proofs, testing,
    ):
        return
    attributes = []
    for node in container.named_children:
        if node.type in {"line_comment", "block_comment"}:
            continue
        if node.type == "attribute_item":
            attributes.append(node)
            continue
        selected = not excluded(attributes, exclude_proofs, testing)
        if selected:
            yield node, attributes
        attributes = []


def resolve_module_file(root, path, file_module, module, name, attributes, snapshot=None):
    explicit = [
        match[1]
        for attribute in attributes
        if (match := re.fullmatch(r'#\[\s*path\s*=\s*"([^"]+)"\s*\]', attribute.text.decode("utf-8")))
    ]
    if len(explicit) > 1:
        raise ValueError(f"duplicate module path attributes: {module}::{name}")
    directory = path.parent
    suffix = module[len(file_module):].strip(":")
    if file_module and path.name not in {"lib.rs", "mod.rs"} and (not explicit or suffix):
        directory /= path.stem
    if suffix:
        directory = directory.joinpath(*suffix.split("::"))
    candidates = (
        [directory / explicit[0]] if explicit
        else [directory / f"{name}.rs", directory / name / "mod.rs"]
    )
    existing = [
        candidate for candidate in candidates
        if (
            candidate.is_file()
            if snapshot is None or not candidate.resolve().is_relative_to(root / "src")
            else candidate.resolve().relative_to(root).as_posix() in snapshot
        )
    ]
    if len(existing) != 1:
        raise ValueError(f"missing or ambiguous module {module}::{name} in {path}")
    selected = existing[0]
    if (
        not selected.resolve().is_relative_to(root.resolve()) or selected.is_symlink()
        or any(parent.is_symlink() for parent in selected.parents if parent.is_relative_to(root))
    ):
        raise ValueError(f"external or symlinked module source: {selected}")
    return selected.resolve()


@lru_cache(maxsize=2048)
def production_code(source: bytes, exclude_proofs=True, mask_noncode=True) -> str:
    """Mask excluded items and non-code without losing byte/line positions."""
    root = parse(source)
    masked = bytearray(source)

    def blank(node):
        for index in range(node.start_byte, node.end_byte):
            if masked[index] not in (10, 13):
                masked[index] = 32

    def proof_statement(node):
        # The grammar can join a brace-macro statement to a following dereference
        # or tuple. A cfg on proof! applies to the macro, not that next statement.
        start = node.start_byte
        end = node.end_byte
        while node.start_byte == start:
            if node.type == "macro_invocation":
                name = node.child_by_field_name("macro")
                if name is not None and name.text in {b"proof", b"proof_decl"}:
                    return node
                if source[node.end_byte:end].strip() not in (b"", b";"):
                    raise ValueError("ambiguous cfg-gated macro statement needs audit support")
                return None
            if not node.named_children:
                break
            node = node.named_children[0]
        return None

    def visit(node):
        if excluded(
            [child for child in node.named_children if child.type == "inner_attribute_item"],
            exclude_proofs,
        ):
            blank(node)
            return
        attributes = []
        for child in node.named_children:
            if child.type in {"line_comment", "block_comment", "string_literal",
                              "raw_string_literal", "char_literal"}:
                if mask_noncode:
                    blank(child)
            elif child.type == "attribute_item":
                attributes.append(child)
            else:
                if excluded(attributes, exclude_proofs):
                    for attribute in attributes:
                        blank(attribute)
                    statement = proof_statement(child)
                    if statement is not None and statement.end_byte < child.end_byte:
                        blank(statement)
                        visit(child)
                    else:
                        blank(child)
                else:
                    visit(child)
                attributes = []

    visit(root)
    return masked.decode("utf-8")


def type_name(node) -> str:
    if node is None:
        raise ValueError("missing impl/trait owner")
    if node.type == "generic_type":
        return type_name(node.child_by_field_name("type"))
    if node.type == "scoped_type_identifier":
        return type_name(node.child_by_field_name("name"))
    return node.text.decode("utf-8")


def source_functions(root: Path) -> list[dict]:
    root = root.resolve()
    src = root / "src"
    paths = sorted(src.rglob("*"))
    if (
        src.is_symlink() or not (src / "lib.rs").is_file()
        or any(path.is_symlink() for path in paths)
        or any(path.is_file() and path.suffix != ".rs" for path in paths)
    ):
        return _source_functions(root)
    snapshot = tuple(
        (path.relative_to(root).as_posix(), path.read_bytes())
        for path in paths if path.is_file()
    )
    return deepcopy(_source_functions_snapshot(root, snapshot))


@lru_cache(maxsize=8)
def _source_functions_snapshot(root, snapshot):
    return _source_functions(root, dict(snapshot))


def source_modules(root: Path) -> dict[str, str]:
    root = root.resolve()
    paths = sorted((root / "src").rglob("*"))
    if (
        (root / "src").is_symlink() or not (root / "src/lib.rs").is_file()
        or any(path.is_symlink() or path.is_file() and path.suffix != ".rs" for path in paths)
    ):
        modules = {}
        _source_functions(root, module_catalog=modules)
        return modules
    snapshot = tuple(
        (path.relative_to(root).as_posix(), path.read_bytes())
        for path in paths if path.is_file()
    )
    return deepcopy(_source_modules_snapshot(root, snapshot))


@lru_cache(maxsize=8)
def _source_modules_snapshot(root, snapshot):
    modules = {}
    _source_functions(root, dict(snapshot), module_catalog=modules)
    return modules


def _source_functions(root: Path, snapshot=None, module_catalog=None) -> list[dict]:
    import hashlib

    root = root.resolve()
    src = root / "src"
    result = []
    visited = {}
    file_hashes = {}
    definitions = {}

    def visit_file(path, module):
        path = path.resolve()
        if not path.is_relative_to(src.resolve()):
            raise ValueError(f"runtime module outside src: {path}")
        if path in visited:
            raise ValueError(f"runtime module included more than once: {path}")
        visited[path] = module
        source = path.read_bytes() if snapshot is None else snapshot[path.relative_to(root).as_posix()]
        file_hashes[path] = hashlib.sha256(source).hexdigest()
        walk(parse(source), module, path, None, False)

    def walk(container, module, path, owner, verify_impl, impl_trait=None):
        for node, attributes in items(container):
            if node.type in {"function_item", "function_signature_item"}:
                def nested_function(child):
                    return any(
                        grandchild.type == "function_item" or nested_function(grandchild)
                        for grandchild in child.named_children
                    )
                if nested_function(node):
                    raise ValueError("local function declarations need explicit audit support")
                name = node.child_by_field_name("name").text.decode("utf-8")
                visibility = next(
                    (child.text.decode("utf-8") for child in node.named_children
                     if child.type == "visibility_modifier"), "private",
                )
                identity_parts = path.relative_to(src.resolve()).with_suffix("").parts
                if path.name == "mod.rs":
                    identity_parts = identity_parts[:-1]
                identity = "::".join(identity_parts)
                inline_module = module[len(visited[path]):].strip(":")
                if inline_module:
                    identity += f"::{inline_module}"
                result.append({
                    "id": f"{identity}::{owner + '::' if owner else ''}{name}",
                    "file": path.relative_to(root).as_posix(),
                    "visibility": visibility,
                    "owner": owner,
                    "impl_trait": impl_trait,
                    "verifier_module": module,
                    "verification_scope": module,
                    "file_module": visited[path],
                    "name": name,
                    "line": node.start_point.row + 1,
                    "attributes": [attribute.text.decode("utf-8") for attribute in attributes],
                    "body_text": node.text.decode("utf-8"),
                    "file_sha256": file_hashes[path],
                    "verification_requested": verify_impl or any(
                        re.search(rb"\b(verus_verify|verus_spec)\b", attribute.text)
                        for attribute in attributes
                    ),
                    "runtime_proof_divergence": bool(re.search(
                        r"#\[\s*cfg\s*\([^]]*\bnot\s*\(\s*verus_keep_ghost\s*\)",
                        production_code(node.text),
                    )),
                })
            elif node.type in {"impl_item", "trait_item"}:
                target = node.child_by_field_name(
                    "type" if node.type == "impl_item" else "name"
                )
                body = node.child_by_field_name("body")
                if body is not None:
                    trait = node.child_by_field_name("trait") if node.type == "impl_item" else None
                    walk(body, module, path, type_name(target), any(
                        re.search(rb"\bverus_verify\b", attribute.text)
                        for attribute in attributes
                    ), "".join(trait.text.decode("utf-8").split()) if trait is not None else None)
            elif node.type in {"struct_item", "enum_item", "type_item"}:
                name = node.child_by_field_name("name").text.decode("utf-8")
                definitions.setdefault(name, set()).add(module)
            elif node.type == "mod_item":
                name = node.child_by_field_name("name").text.decode("utf-8")
                nested = f"{module}::{name}" if module else name
                body = node.child_by_field_name("body")
                if body is not None:
                    walk(body, nested, path, None, False)
                    continue
                visit_file(resolve_module_file(
                    root, path, visited[path], module, name, attributes, snapshot,
                ), nested)
            elif node.type in {"macro_definition", "macro_invocation"} and re.search(
                r"\bfn\s+\w", production_code(node.text)
            ):
                raise ValueError("macro-generated functions need explicit audit support")

    visit_file(src / "lib.rs", "")
    if module_catalog is not None:
        module_catalog.update({
            path.relative_to(root).as_posix(): module for path, module in visited.items()
        })
    for entry in result:
        modules = definitions.get(entry["owner"], set())
        if len(modules) == 1:
            entry["verifier_module"] = next(iter(modules))
        elif len(modules) > 1 and entry["verifier_module"] not in modules:
            raise ValueError(f"ambiguous owner definition: {entry['id']}")
    # Stable source order preserves duplicate trait-method disposition IDs.
    return sorted(result, key=lambda entry: entry["file"])
