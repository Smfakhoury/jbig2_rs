"""Reconcile pinned compiler item metadata, executable MIR and source identities."""

from collections import Counter, defaultdict
import hashlib
import re
from pathlib import PurePosixPath

from check_traceability import runtime_inventory
from rust_source import items, parse, source_modules


RUSTDOC_FORMAT = 61
MIR_FUNCTION = re.compile(r"^fn ([^\n]+) \{\n(.*?)^\}", re.M | re.S)
MIR_IMPL = re.compile(r"<impl at (src/[^:]+):(\d+):(\d+):\s*\d+:\d+>::(\w+)$")
MIR_CLOSURE_SPAN = re.compile(r"\{closure@(src/[^:}]+):(\d+):(\d+):\s*(\d+):(\d+)\}")
DERIVE_METHODS = {
    "Clone": "clone", "Debug": "fmt", "Default": "default",
    "Eq": "assert_fields_are_eq", "PartialEq": "eq",
}


def source_span(item):
    span = item["span"]
    if not isinstance(span, dict):
        raise ValueError("owned compiler item lacks a source span")
    path = PurePosixPath(span["filename"])
    if path.parts[:1] != ("src",) or ".." in path.parts or path.suffix != ".rs":
        raise ValueError("owned compiler item has an out-of-scope source span")
    for point in (span["begin"], span["end"]):
        if len(point) != 2 or any(type(value) is not int or value < 1 for value in point):
            raise ValueError("invalid compiler source coordinates")
    if tuple(span["end"]) < tuple(span["begin"]):
        raise ValueError("reversed compiler source span")
    return span


def compiler_path(document, item):
    path = document["paths"].get(str(item["id"]))
    if (
        not isinstance(path, dict) or path["crate_id"] != 0
        or path["path"][:1] != ["jbig2_decoder"] or len(path["path"]) < 2
    ):
        raise ValueError("owned compiler item lacks an exact crate-qualified path")
    return "::".join(path["path"][1:])


def source_datatypes(root):
    datatypes, derives, constructors = set(), set(), set()

    def has_fields(container):
        for node, _ in items(container):
            if node.type == "field_declaration":
                return True
            if node.type == "ordered_field_declaration_list" and any(
                child.type not in {"line_comment", "block_comment", "attribute_item"}
                for child, _ in items(node)
            ):
                return True
            if has_fields(node):
                return True
        return False

    def walk(container, file):
        for node, attributes in items(container):
            if node.type == "mod_item":
                body = node.child_by_field_name("body")
                if body is not None:
                    walk(body, file)
            if node.type not in {"struct_item", "enum_item"}:
                continue
            name = node.child_by_field_name("name").text.decode()
            datatypes.add((file, node.start_point.row + 1, name))
            body = node.child_by_field_name("body")
            candidates = [(node, body)] if node.type == "struct_item" else [
                (variant, variant.child_by_field_name("body"))
                for variant, _ in items(body) if variant.type == "enum_variant"
            ]
            for declaration, fields in candidates:
                if fields is not None and fields.type == "ordered_field_declaration_list":
                    constructors.add((
                        file, declaration.start_point.row + 1,
                        declaration.child_by_field_name("name").text.decode(),
                    ))
            tokens = []
            for attribute in attributes:
                if not re.match(rb"#\[\s*derive\s*\(", attribute.text):
                    continue
                arguments = attribute.named_children[0].child_by_field_name("arguments")
                if arguments is None:
                    raise ValueError("unparsed source derive arguments")
                for token in arguments.named_children:
                    if token.type in {"line_comment", "block_comment"}:
                        continue
                    trait = token.text.decode()
                    if token.type != "identifier" or trait not in {*DERIVE_METHODS, "Copy"}:
                        raise ValueError("new source derive form needs explicit compiler qualification")
                    tokens.append(token)
            copy = any(token.text == b"Copy" for token in tokens)
            for token in tokens:
                trait = token.text.decode()
                method = "marker:" + trait if trait == "Copy" or (
                    trait == "Eq" and not has_fields(node)
                ) else DERIVE_METHODS[trait]
                methods = [method]
                if trait == "PartialEq":
                    methods.append("marker:StructuralPartialEq")
                if trait == "Clone" and copy:
                    methods.append("marker:TrivialClone")
                for method in methods:
                    derives.add((
                        file, token.start_point.row + 1, token.start_point.column + 1,
                        method, name,
                    ))
    for file in source_modules(root):
        walk(parse((root / file).read_bytes()), file)
    return datatypes, derives, constructors


def check_derive_source(root, document, item, owner):
    span = source_span(item)
    if source_span(owner) != span:
        raise ValueError("derived function and implementation have different source owners")
    trait = owner["inner"]["impl"]["trait"]["path"]
    if DERIVE_METHODS.get(trait) != item["name"]:
        raise ValueError(f"unqualified generated method: {trait}::{item['name']}")
    source = (root / span["filename"]).read_text()
    lines = source.splitlines(keepends=True)
    if span["begin"][0] != span["end"][0] or span["begin"][0] > len(lines):
        raise ValueError("derived method lacks a precise source derive token")
    line = lines[span["begin"][0] - 1]
    start, end = span["begin"][1] - 1, span["end"][1] - 1
    if line[start:end] != trait:
        raise ValueError("generated method source does not name its actual derive rule")
    offset = len(("".join(lines[:span["begin"][0] - 1]) + line[:start]).encode())
    node = parse(source.encode()).descendant_for_byte_range(offset, offset + len(trait) - 1)
    while node is not None and node.type != "attribute_item":
        node = node.parent
    if node is None or not re.match(rb"#\[\s*derive\s*\(", node.text):
        raise ValueError("generated method source is not an actual derive attribute")
    declaration = node.next_named_sibling
    while declaration is not None and declaration.type in {
        "attribute_item", "line_comment", "block_comment",
    }:
        declaration = declaration.next_named_sibling
    target = owner["inner"]["impl"]["for"].get("resolved_path")
    if (
        declaration is None or declaration.type not in {"struct_item", "enum_item"}
        or target is None or str(target["id"]) not in document["index"]
        or declaration.child_by_field_name("name").text.decode() != document["index"][str(target["id"])]["name"]
        or declaration.start_point.row + 1 != document["index"][str(target["id"])]["span"]["begin"][0]
    ):
        raise ValueError("derived implementation targets a different source datatype")


def mir_bodies(text):
    if not text.startswith("// WARNING: This output format is intended for human consumers only\n"):
        raise ValueError("unrecognized pinned MIR format")
    matches = list(MIR_FUNCTION.finditer(text))
    if not matches or len(matches) != len(re.findall(r"^fn ", text, re.M)):
        raise ValueError("missing, truncated or unparsed MIR function bodies")
    bodies, ctfe = {}, []
    for match in matches:
        signature, body = match.groups()
        name = signature.split("(", 1)[0]
        if not name or not body.strip() or "bb0:" not in body:
            raise ValueError("empty or malformed compiler MIR body")
        previous = text[max(0, match.start() - 50):match.start()].rstrip()
        if previous.endswith("// MIR FOR CTFE"):
            if name not in bodies or name in ctfe:
                raise ValueError("CTFE body lacks one corresponding executable identity")
            ctfe.append(name)
        else:
            if name in bodies:
                raise ValueError("duplicate executable compiler MIR identity")
            bodies[name] = {
                "signature": signature,
                "body_sha256": hashlib.sha256(body.encode()).hexdigest(),
                "line": text.count("\n", 0, match.start()) + 1,
            }
    return bodies, ctfe


def compiler_items(document):
    if (
        document["format_version"] != RUSTDOC_FORMAT or document["includes_private"] is not True
        or document["index"][str(document["root"])]["name"] != "jbig2_decoder"
    ):
        raise ValueError("wrong compiler schema, crate or missing private-item export")
    index = document["index"]
    parents = defaultdict(list)
    for key, item in index.items():
        if str(item["id"]) != key or len(item["inner"]) != 1:
            raise ValueError("ambiguous compiler item identity")
        for kind in ("module", "impl", "trait", "enum"):
            if kind in item["inner"]:
                value = item["inner"][kind]
                for child in value.get("items", value.get("variants", [])):
                    if str(child) not in index:
                        raise ValueError("compiler owner references a missing item")
                    parents[str(child)].append(item)
    functions, constructors = [], []
    for item in index.values():
        if item["crate_id"] != 0:
            continue
        inner = item["inner"]
        if "function" in inner:
            source_span(item)
            functions.append(item)
        elif (
            "variant" in inner and isinstance(inner["variant"]["kind"], dict)
            and "tuple" in inner["variant"]["kind"]
        ) or (
            "struct" in inner and isinstance(inner["struct"]["kind"], dict)
            and "tuple" in inner["struct"]["kind"]
        ):
            source_span(item)
            compiler_path(document, item)
            constructors.append(item)
    if not functions:
        raise ValueError("compiler function inventory is empty")
    return functions, constructors, parents


def source_root_exports(root):
    exports = []

    def path(node):
        if node.type in {"identifier", "crate", "self", "super"}:
            return (node.text.decode(),)
        if node.type == "scoped_identifier":
            return path(node.child_by_field_name("path")) + path(node.child_by_field_name("name"))
        raise ValueError("unqualified source reexport path")

    def walk(node, prefix=()):
        if node.type == "scoped_use_list":
            walk(node.child_by_field_name("list"), prefix + path(node.child_by_field_name("path")))
        elif node.type == "use_list":
            for child in node.named_children:
                if child.type not in {"line_comment", "block_comment"}:
                    walk(child, prefix)
        elif node.type == "use_as_clause":
            target = prefix + path(node.child_by_field_name("path"))
            exports.append(("::".join(target), node.child_by_field_name("alias").text.decode()))
        else:
            target = prefix + path(node)
            exports.append(("::".join(target), target[-1]))

    for node, _ in items(parse((root / "src/lib.rs").read_bytes())):
        visibility = next((
            child.text for child in node.named_children if child.type == "visibility_modifier"
        ), None)
        if visibility != b"pub":
            continue
        if node.type != "use_declaration":
            raise ValueError("new source root public declaration needs export-domain review")
        walk(node.child_by_field_name("argument"))
    if len({name for _, name in exports}) != len(exports):
        raise ValueError("ambiguous source root reexports")
    return sorted(exports)


def public_exports(document, root=None):
    index = document["index"]
    result = []
    module = index[str(document["root"])]
    for identity in module["inner"]["module"]["items"]:
        item = index[str(identity)]
        if item["visibility"] != "public":
            continue
        if "use" not in item["inner"]:
            raise ValueError("new public declaration form needs explicit export-domain review")
        use = item["inner"]["use"]
        if use["is_glob"] or str(use["id"]) not in index:
            raise ValueError("public glob or unresolved reexport needs explicit review")
        target = index[str(use["id"])]
        result.append({
            "name": use["name"], "source": use["source"], "compiler_id": target["id"],
            "kind": next(iter(target["inner"])), "source_span": source_span(target),
        })
    if len({item["name"] for item in result}) != len(result):
        raise ValueError("ambiguous root public exports")
    if root is not None and sorted(
        (item["source"], item["name"]) for item in result
    ) != source_root_exports(root):
        raise ValueError("compiler root reexports differ from selected source reexports")
    return sorted(result, key=lambda item: item["name"])


def declaration_surfaces(document, parents, records):
    index = document["index"]
    exports = public_exports(document)
    aliases = defaultdict(list)
    for export in exports:
        if export["kind"] not in {"struct", "enum", "trait", "function"}:
            raise ValueError("new root export kind needs declaration-surface review")
        aliases[export["compiler_id"]].append(export["name"])
    by_mir = {row["mir_identity"]: row for row in records if row.get("mir_identity")}
    for row in records:
        if row["origin"] == "compiler_closure":
            continue
        item = index[str(row["compiler_id"])]
        owners = [
            owner for owner in parents[str(item["id"])]
            if {"impl", "trait", "enum"} & owner["inner"].keys()
        ]
        if len(owners) > 1:
            raise ValueError("ambiguous callable declaration-surface owner")
        owner = owners[0] if owners else None
        target = item["id"]
        category = "non_root_function_or_method"
        if row["origin"] == "compiler_adt_constructor":
            category = "non_root_or_private_adt_constructor"
            if "variant" in item["inner"]:
                if owner is None or "enum" not in owner["inner"]:
                    raise ValueError("variant constructor lacks its owning enum")
                target = owner["id"]
                exposed = bool(aliases[target])
            else:
                fields = item["inner"]["struct"]["kind"]["tuple"]
                exposed = bool(aliases[target]) and all(
                    field is not None and index[str(field)]["visibility"] == "public"
                    for field in fields
                )
            if exposed:
                category = "root_type_adt_constructor"
        elif owner is not None and "impl" in owner["inner"]:
            implementation = owner["inner"]["impl"]
            resolved = implementation["for"].get("resolved_path")
            target = resolved["id"] if resolved else None
            if aliases[target]:
                if implementation["trait"] is not None:
                    category = "trait_implementation_on_root_type"
                elif item["visibility"] == "public":
                    category = "public_inherent_method_on_root_type"
        elif owner is not None and "trait" in owner["inner"]:
            target = owner["id"]
            if aliases[target]:
                category = "root_trait_method"
        elif aliases[item["id"]]:
            category = "root_reexported_function"
        surface = {
            "category": category, "root_aliases": sorted(aliases[target]),
            "owner_compiler_id": owner["id"] if owner else None,
            "target_compiler_id": target,
        }
        if category not in {
            "non_root_function_or_method", "non_root_or_private_adt_constructor",
            "root_type_adt_constructor",
        }:
            function = item["inner"]["function"]
            if any(not isinstance(function.get(key), dict) for key in ("sig", "generics", "header")):
                raise ValueError("API declaration lacks a complete compiler signature")
            surface["compiler_signature"] = {
                key: function[key] for key in ("sig", "generics", "header")
            }
        row["declaration_surface"] = surface
    closures = sorted(
        (row for row in records if row["origin"] == "compiler_closure"),
        key=lambda row: row["mir_identity"].count("::{closure#"),
    )
    for row in closures:
        parent = by_mir[row["parent_mir_identity"]]["declaration_surface"]
        row["declaration_surface"] = {
            "category": "internal_closure_body", "root_aliases": [],
            "enclosing_declaration_category": parent.get(
                "enclosing_declaration_category", parent["category"],
            ),
        }
    return {
        "status": "compiler_root_declaration_surfaces_classified",
        "classified_owned_identities": len(records),
        "categories": dict(sorted(Counter(
            row["declaration_surface"]["category"] for row in records
        ).items())),
        "call_graph_reachability_qualified": False,
        "constructible_argument_domains_qualified": False,
        "claim_boundary": (
            "Declaration exposure in the isolated native compiler profile, not call-graph "
            "reachability or a proof that trait arguments are publicly constructible. "
            "Private helpers and closures can execute under public callers. Compiler signatures "
            "retain generic bounds but do not qualify semantic preconditions or input domains. "
            "Public data fields, associated constants, external blanket implementations and "
            "other compiler/platform/proof profiles are not a complete API/domain inventory."
        ),
    }


def reconcile(root, document, mir_text):
    functions, constructors, parents = compiler_items(document)
    exports = public_exports(document, root)
    bodies, ctfe = mir_bodies(mir_text)
    runtime = runtime_inventory(root)
    datatypes, expected_derives, expected_constructors = source_datatypes(root)
    compiler_datatypes = {
        (source_span(item)["filename"], item["span"]["begin"][0], item["name"])
        for item in document["index"].values()
        if item["crate_id"] == 0 and {"struct", "enum"} & item["inner"].keys()
    }
    if compiler_datatypes != datatypes:
        raise ValueError("compiler datatype inventory omits or invents selected source declarations")
    if {
        (source_span(item)["filename"], item["span"]["begin"][0], item["name"])
        for item in constructors
    } != expected_constructors:
        raise ValueError("compiler constructor inventory omits or invents selected source declarations")
    source_keys = defaultdict(list)
    for identity, record in runtime.items():
        source_keys[(record["file"], record["line"], record["name"])].append(identity)
    methods, free = defaultdict(list), {}
    records, compiler_records, source_matches, observed_derives = [], {}, set(), set()
    marker_impls = []
    for owner in document["index"].values():
        if (
            owner["crate_id"] == 0 and "impl" in owner["inner"]
            and "automatically_derived" in owner["attrs"]
            and not owner["inner"]["impl"]["items"]
        ):
            span = source_span(owner)
            implementation = owner["inner"]["impl"]
            trait = implementation["trait"]["path"]
            if trait not in {"Copy", "Eq", "TrivialClone", "StructuralPartialEq"}:
                raise ValueError("unqualified empty generated implementation")
            target = implementation["for"]["resolved_path"]
            name = document["index"][str(target["id"])]["name"]
            observed_derives.add((span["filename"], *span["begin"], "marker:" + trait, name))
            marker_impls.append({
                "compiler_id": owner["id"], "source_span": span,
                "trait": trait, "target": name, "origin": "compiler_marker_implementation",
                "proof_qualification": "no_executable_method_body_or_separate_proof_credit",
            })
    for item in functions:
        span = source_span(item)
        owners = parents[str(item["id"])]
        impls = [owner for owner in owners if "impl" in owner["inner"]]
        matches = source_keys[(span["filename"], span["begin"][0], item["name"])]
        if len(matches) > 1 or len(impls) > 1:
            raise ValueError("compiler source span aliases distinct declarations")
        derived = bool(impls and "automatically_derived" in impls[0]["attrs"])
        if matches:
            identity = matches[0]
            if identity in source_matches or derived:
                raise ValueError("duplicate or misclassified explicit compiler declaration")
            trait_method = any("trait" in owner["inner"] for owner in owners) or bool(
                impls and impls[0]["inner"]["impl"]["trait"] is not None
            )
            if not trait_method and (
                (item["visibility"] == "public") != (runtime[identity]["visibility"] == "pub")
            ):
                raise ValueError("compiler public function visibility differs from selected source")
            source_matches.add(identity)
            kind = "explicit_source_declaration"
        elif derived:
            check_derive_source(root, document, item, impls[0])
            target = impls[0]["inner"]["impl"]["for"]["resolved_path"]
            observed_derives.add((
                span["filename"], *span["begin"], item["name"],
                document["index"][str(target["id"])]["name"],
            ))
            identity = "derive:" + ":".join(map(str, (
                span["filename"], *span["begin"], item["name"],
            )))
            kind = "compiler_derived_method"
        else:
            raise ValueError("unmapped compiler-owned function is not an audited derive")
        owner = impls[0] if impls else None
        trait = owner["inner"]["impl"]["trait"] if owner else None
        row = {
            "id": identity, "compiler_id": item["id"], "name": item["name"],
            "source_span": span, "origin": kind,
            "visibility": item["visibility"],
            "impl_trait": trait["path"] if trait else None,
            "compiler_has_body": item["inner"]["function"]["has_body"],
            "source_id": matches[0] if matches else None,
            "mir_identity": None, "ctfe_body": False,
            "proof_qualification": "not_inferred_from_compiler_inventory",
            "entry_domain": "source_contract_domain_requires_separate_qualification",
        }
        records.append(row)
        compiler_records[item["id"]] = row
        if owner:
            owner_span = source_span(owner)
            methods[(owner_span["filename"], *owner_span["begin"], item["name"])].append(item["id"])
        elif item["inner"]["function"]["has_body"]:
            path = compiler_path(document, item)
            if path in free:
                raise ValueError("duplicate free compiler function path")
            free[path] = item["id"]
    if source_matches != set(runtime):
        raise ValueError("compiler export omits selected explicit production declarations")
    if observed_derives != expected_derives:
        raise ValueError("compiler export omits or invents source-derived methods")
    constructor_paths = {}
    for item in constructors:
        path = compiler_path(document, item)
        if path in constructor_paths:
            raise ValueError("duplicate compiler ADT constructor path")
        constructor_paths[path] = item
    matched, constructor_matches, closures = set(), set(), []
    for name, body in bodies.items():
        if "{closure#" in name:
            parent = name.rsplit("::{closure#", 1)[0]
            if parent not in bodies:
                raise ValueError("compiled closure lacks its exact parent body")
            location = MIR_CLOSURE_SPAN.search(body["signature"])
            if location is None:
                raise ValueError("compiled closure lacks an exact compiler source span")
            file, first_line, first_column, last_line, last_column = location.groups()
            span = source_span({"span": {
                "filename": file, "begin": [int(first_line), int(first_column)],
                "end": [int(last_line), int(last_column)],
            }})
            if not (root / file).is_file():
                raise ValueError("compiled closure references absent source")
            closures.append({
                "id": "closure:" + name, "mir_identity": name, "parent_mir_identity": parent,
                "origin": "compiler_closure", "source_span": span, **body,
                "proof_qualification": "owning_contract_and_call_domain_not_yet_qualified",
            })
            continue
        impl = MIR_IMPL.search(name)
        choices = []
        if impl:
            file, line, column, method = impl.groups()
            choices = methods[(file, int(line), int(column), method)]
        elif name in free:
            choices = [free[name]]
        elif name in constructor_paths:
            item = constructor_paths[name]
            constructor_matches.add(name)
            records.append({
                "id": "constructor:" + name, "compiler_id": item["id"],
                "name": item["name"], "source_span": source_span(item),
                "origin": "compiler_adt_constructor", "mir_identity": name,
                "proof_qualification": "language_constructor_semantics_not_separate_proof_credit",
                **body,
            })
            continue
        if len(choices) != 1 or choices[0] in matched:
            raise ValueError(f"unmapped or ambiguous executable compiler identity: {name}")
        matched.add(choices[0])
        row = compiler_records[choices[0]]
        row.update(mir_identity=name, ctfe_body=name in ctfe, **body)
    if constructor_matches != set(constructor_paths):
        raise ValueError("compiler ADT constructor inventory differs from executable MIR")
    for item in functions:
        if (item["id"] in matched) != item["inner"]["function"]["has_body"]:
            raise ValueError("compiler declaration/body inventory differs from executable MIR")
    records.extend(closures)
    if len({record["id"] for record in records}) != len(records):
        raise ValueError("compiled inventory source identities collide")
    surfaces = declaration_surfaces(document, parents, records)
    return {
        "schema_version": 1, "status": "compiler_native_profile_inventory_reconciled",
        "rustdoc_format": RUSTDOC_FORMAT,
        "counts": {
            "explicit_source_declarations": len(runtime),
            "compiler_function_items": len(functions),
            "compiler_derived_methods": sum(row["origin"] == "compiler_derived_method" for row in records),
            "abstract_declarations": len(functions) - len(matched),
            "executable_mir_bodies": len(bodies), "closures": len(closures),
            "adt_constructors": len(constructor_paths), "separate_ctfe_bodies": len(ctfe),
            "owned_callable_and_abstract_identities": len(records),
            "compiler_marker_implementations": len(marker_impls),
        },
        "derived_traits": dict(sorted(Counter(
            row["impl_trait"] for row in records if row["origin"] == "compiler_derived_method"
        ).items())),
        "public_root_exports": exports,
        "api_declaration_surfaces": surfaces,
        "marker_implementations": sorted(marker_impls, key=lambda row: row["compiler_id"]),
        "records": sorted(records, key=lambda row: row["id"]),
        "ready_for_external_validation": False,
        "complete_production_inventory_qualified": False,
        "semantic_contract_quality_qualified": False,
        "public_entry_reachability_qualified": False,
        "claim_boundary": (
            "Actual isolated pinned native compiler profile: source declarations, derives, "
            "closures and ADT constructor bodies, with CTFE duplicates separate. Counts are not "
            "proof/contract percentages. Root exports are not call-graph reachability. Proof-mode/"
            "platform reconciliation, generated contract ownership and semantic entry domains "
            "remain separate obligations; no existing source declaration or generated item is hidden."
        ),
    }
