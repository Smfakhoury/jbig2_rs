"""Bounded ownership audit for existing, pinned FromSpec result contracts."""

import hashlib
import re
import tomllib

from implementation_identity import function_name, validate_case
from rust_source import items, parse, production_code


VSTD_VERSION = "0.0.0-2026-10-04-0306"
VSTD_CHECKSUM = "f22e1cad7b5f35642a5a020b25e9b49915066d836bab77f8431bbc80228290a7"
FROM_SOURCE_SHA256 = "a8ecf63aa62cb0502ea22750a892fc3e4d884842d2b1ca6a842c2d2f8f5651e7"


def standard_from_contract(root):
    manifest_path = root / "verification/Cargo.toml"
    lock_path = root / "verification/Cargo.lock"
    manifest = tomllib.loads(manifest_path.read_text())
    packages = [
        package for package in tomllib.loads(lock_path.read_text())["package"]
        if package["name"] == "vstd"
    ]
    if (
        manifest["dependencies"].get("vstd") != "=" + VSTD_VERSION
        or len(packages) != 1 or packages[0]["version"] != VSTD_VERSION
        or packages[0]["checksum"] != VSTD_CHECKSUM
        or packages[0]["source"] != "registry+https://github.com/rust-lang/crates.io-index"
    ):
        raise ValueError("From contract ownership requires the audited existing vstd package pin")
    return {
        "package": "vstd", "version": VSTD_VERSION, "package_sha256": VSTD_CHECKSUM,
        "source_file": "std_specs/convert.rs", "source_file_sha256": FROM_SOURCE_SHA256,
        "contract": "vstd::std_specs::convert::ExFrom::from",
        "condition": "FromSpec::obeys_from_spec",
        "result_model": "FromSpec::from_spec",
        "manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "lock_sha256": hashlib.sha256(lock_path.read_bytes()).hexdigest(),
        "binding_scope": (
            "Fixed source-rule binding at the existing package pin, checked against the manifest/"
            "lock. No fresh dependency-source acquisition, new assumption or independent approval."
        ),
    }


def compact(text):
    return re.sub(r"\s+", "", text)


def model_inventory(root, runtime):
    models = {}
    files = {}
    for record in runtime.values():
        files.setdefault(record["file"], set()).add(record["file_module"])
    for relative, modules in files.items():
        if len(modules) != 1:
            raise ValueError("ownership model file has ambiguous lexical module")
        source = (root / relative).read_bytes()
        masked = production_code(source, exclude_proofs=False).encode()

        def walk(container, scope):
            entries = list(items(container, exclude_proofs=False))
            imports = [
                compact(masked[node.start_byte:node.end_byte].decode())
                for node, _ in entries if node.type == "use_declaration"
            ]
            imported = any(
                (
                    declaration == "usevstd::std_specs::convert::FromSpecImpl;"
                    or (
                        declaration.startswith("usevstd::std_specs::convert::{")
                        and "FromSpecImpl" in declaration.removeprefix(
                            "usevstd::std_specs::convert::{"
                        ).removesuffix("};").split(",")
                    )
                )
                for declaration in imports
            )
            for node, _ in entries:
                if node.type == "mod_item" and node.child_by_field_name("body") is not None:
                    name = node.child_by_field_name("name").text.decode()
                    walk(node.child_by_field_name("body"), scope + "::" + name if scope else name)
                if node.type != "macro_invocation" or node.child_by_field_name("macro").text != b"verus":
                    continue
                tree = next(child for child in node.named_children if child.type == "token_tree")
                for index, token in enumerate(tree.children):
                    if token.type != "impl":
                        continue
                    body = next((
                        child for child in tree.children[index + 1:]
                        if child.type == "token_tree" and child.text.startswith(b"{")
                    ), None)
                    if body is None:
                        raise ValueError("unterminated ownership model implementation")
                    header = masked[token.start_byte:body.start_byte].decode()
                    match = re.fullmatch(
                        r"\s*impl\s+(?P<qualified>vstd\s*::\s*std_specs\s*::\s*convert\s*::\s*)?"
                        r"FromSpecImpl\s*<\s*(?P<argument>\w+)\s*>\s+for\s+(?P<owner>\w+)\s*",
                        header,
                    )
                    if match is None:
                        continue
                    if match["qualified"] is None and not imported:
                        raise ValueError("unresolved FromSpecImpl import cannot own a standard contract")
                    start, end = token.start_byte, body.end_byte
                    normalized = bytearray(source[start:end])
                    for child in body.children:
                        if child.type == "identifier" and child.text in {b"open", b"closed", b"spec"}:
                            normalized[child.start_byte - start:child.end_byte - start] = (
                                b" " * (child.end_byte - child.start_byte)
                            )
                    declaration = next(
                        child for child in parse(bytes(normalized)).named_children
                        if child.type == "impl_item"
                    )
                    method_nodes = [
                        child for child, _ in items(declaration.child_by_field_name("body"))
                        if child.type == "function_item"
                    ]
                    methods = {
                        child.child_by_field_name("name").text.decode(): child
                        for child in method_nodes
                    }
                    if len(method_nodes) != 2 or set(methods) != {"obeys_from_spec", "from_spec"}:
                        raise ValueError("From ownership needs concrete enabling and result methods")
                    enabled, result = methods["obeys_from_spec"], methods["from_spec"]
                    enabled_body = enabled.child_by_field_name("body")
                    result_body = result.child_by_field_name("body")
                    if (
                        enabled.child_by_field_name("parameters").named_children
                        or enabled.child_by_field_name("return_type") is None
                        or enabled.child_by_field_name("return_type").text != b"bool"
                        or enabled_body is None or result_body is None or compact(
                        masked[start + enabled_body.start_byte:start + enabled_body.end_byte].decode()
                        ) != "{true}"
                    ):
                        raise ValueError("From ownership requires a literal enabled model")
                    parameters = result.child_by_field_name("parameters").named_children
                    if len(parameters) != 1 or parameters[0].type != "parameter":
                        raise ValueError("From result model needs one typed input")
                    pattern = parameters[0].child_by_field_name("pattern")
                    argument = parameters[0].child_by_field_name("type")
                    returns = result.child_by_field_name("return_type")
                    if (
                        pattern is None or pattern.type != "identifier" or argument is None
                        or argument.text.decode() != match["argument"] or returns is None
                        or returns.text.decode() not in {"Self", match["owner"]}
                    ):
                        raise ValueError("From model signature does not match the implementation")
                    pending, input_seen = [result_body], False
                    while pending:
                        current = pending.pop()
                        input_seen |= current.type == "identifier" and current.text == pattern.text
                        pending.extend(current.named_children)
                    code = masked[start:end]
                    if not input_seen or re.search(
                        rb"\b(arbitrary|assume|admit|external|external_body|uninterp)\b", code,
                    ):
                        raise ValueError("opaque or input-free From model needs separate ownership review")
                    key = (relative, scope, match["owner"], match["argument"])
                    if key in models:
                        raise ValueError("duplicate From model ownership identity")
                    prefix = source[start:start + enabled_body.end_byte].decode()
                    models[key] = {
                        "file": relative, "scope": scope, "owner": match["owner"],
                        "argument": match["argument"],
                        "line": source[:start].count(b"\n") + 1,
                        "file_sha256": hashlib.sha256(source).hexdigest(),
                        "model_sha256": hashlib.sha256(source[start:end]).hexdigest(),
                        "result_definition_sha256": hashlib.sha256(
                            source[start + result_body.start_byte:start + result_body.end_byte]
                        ).hexdigest(),
                        "enable_prefix": prefix,
                    }
        walk(parse(source), next(iter(modules)))
    return models


def qualify(root, runtime, assessment, reports, configured_cases, whole=None):
    declarations = assessment["code"]["declarations"]
    if {row["id"] for row in declarations} != set(runtime) or len(declarations) != len(runtime):
        raise ValueError("ownership assessment must cover the exact production inventory")
    candidates = [
        row for row in declarations
        if row["accounting_status"] == "verified"
        and row["contract_surface"] != "local_postcondition_surface"
        and row["impl_trait"] is not None and re.fullmatch(r"From<\w+>", row["impl_trait"])
    ]
    if not candidates:
        return {
            "schema_version": 1, "status": "bounded_from_contract_ownership_checked",
            "qualified_existing_owners": 0, "owners": [],
            "remaining_verified_ownership_review": assessment["code"]["no_local_postcondition_by_accounting_status"].get("verified", 0),
            "ready_for_external_validation": False,
        }
    standard = standard_from_contract(root)
    models = model_inventory(root, runtime)
    owners = []
    for row in candidates:
        record = runtime[row["id"]]
        argument = re.fullmatch(r"From<(\w+)>", record["impl_trait"])[1]
        model = models.get((record["file"], record["verification_scope"], record["owner"], argument))
        if model is None:
            raise ValueError(f"missing exact enabled From model owner: {row['id']}")
        controls = []
        for group, document in reports.items():
            recipes = configured_cases.get(group, {})
            for case in document["counterexamples"]:
                if (
                    case.get("source_file") != record["file"]
                    or case.get("verification_module") != record["verification_scope"]
                    or case.get("function") != function_name(record)
                    or case.get("impl_trait") != record["impl_trait"]
                ):
                    continue
                peers = validate_case(runtime, record, case, document, whole, allow_unique_whole=True)
                recipe = recipes.get(case["id"])
                if recipe is None:
                    raise ValueError("ownership evidence has no source-bound configured control")
                before, after = recipe["controls"][0]
                expected = compact(model["enable_prefix"])
                if compact(before) != expected or compact(after) != expected[:-5] + "false}":
                    raise ValueError("weaker control does not disable the exact owning From model")
                controls.append({"group": group, "case_id": case["id"], "selected_impl_traits": peers})
        if not controls:
            raise ValueError(f"From owner lacks exact enabled-contract mutation evidence: {row['id']}")
        owners.append({
            "id": row["id"], "file": record["file"], "line": record["line"],
            "impl_trait": record["impl_trait"],
            "status": "existing_conditional_trait_result_contract_owned",
            "standard_contract": standard["contract"],
            "model": {key: value for key, value in model.items() if key != "enable_prefix"},
            "semantic_controls": controls,
        })
    count = assessment["code"]["no_local_postcondition_by_accounting_status"]["verified"]
    return {
        "schema_version": 1, "status": "bounded_from_contract_ownership_checked",
        "standard_contract": standard,
        "qualified_existing_owners": len(owners), "owners": owners,
        "remaining_verified_ownership_review": count - len(owners),
        "qualification_boundary": (
            "Ownership of the pinned conditional result contract, concrete enabled project model "
            "and actual selective semantic evidence. Model syntax is checked, not its transitive "
            "dependency closure or complete semantic correctness, normative fidelity, reachability, "
            "generated-code coverage or statement completeness."
        ),
        "ready_for_external_validation": False,
    }
