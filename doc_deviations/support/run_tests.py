#!/usr/bin/env python3
"""Replay packaged deviation tests using only the Python standard library.

Recipes execute native build/test commands in temporary copies, not a sandbox.
Review the recipes before running them.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
from hashlib import sha256
from pathlib import Path
from tempfile import TemporaryDirectory, TemporaryFile
from time import monotonic

IGNORED = (
    ".git", ".deepstar", ".venv", "__pycache__", ".pytest-tmp", "doc_deviations")
TAIL_BYTES = 4096


def _clone_project(root: Path, dest: Path) -> None:
    if not root.is_dir():
        raise ValueError(f"repository directory does not exist: {root}")
    if not (root / ".git").exists():
        shutil.copytree(root, dest, symlinks=True,
                        ignore=shutil.ignore_patterns(*IGNORED))
        return
    exclusions = [
        pattern for name in IGNORED
        for pattern in (f":(exclude,glob)**/{name}", f":(exclude,glob)**/{name}/**")
    ]
    manifest = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others",
         "--exclude-standard", "--", ".", *exclusions],
        check=True, stdout=subprocess.PIPE).stdout
    dest.mkdir(parents=True)
    for name in dict.fromkeys(manifest.split(b"\0")):
        if not name:
            continue
        relative = Path(os.fsdecode(name))
        if any(part in IGNORED for part in relative.parts):
            continue
        source, target = root / relative, dest / relative
        if source.is_symlink():
            target.parent.mkdir(parents=True, exist_ok=True)
            target.symlink_to(os.readlink(source))
        elif source.is_dir():
            shutil.copytree(source, target, symlinks=True,
                            ignore=shutil.ignore_patterns(*IGNORED))
        elif source.exists():
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)


def _place_files(workspace: Path, files: list[tuple[str, bytes]]) -> list[str]:
    errors = []
    for path, content in files:
        target = (workspace / path).resolve()
        if not target.is_relative_to(workspace.resolve()):
            errors.append(f"file path escapes the workspace: {path}")
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    return errors


def _run_command(argv: list[str], cwd: Path, timeout_s: float) -> dict:
    env = {key: os.environ[key] for key in ("PATH", "HOME", "TMPDIR") if key in os.environ}
    env["GIT_CEILING_DIRECTORIES"] = str(cwd.resolve().parent)
    started = monotonic()
    code, timed_out = None, False
    with TemporaryFile() as log:
        try:
            with subprocess.Popen(
                    argv, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT,
                    start_new_session=(os.name == "posix")) as process:
                try:
                    code = process.wait(timeout=timeout_s)
                except subprocess.TimeoutExpired:
                    timed_out = True
                    if os.name == "posix":
                        try:
                            os.killpg(process.pid, signal.SIGKILL)
                        except ProcessLookupError:
                            pass  # The command exited between the timeout and termination.
                    else:
                        process.kill()
                    process.wait()
        except OSError as exc:
            log.write(str(exc).encode("utf-8"))
        log.seek(0, os.SEEK_END)
        log.seek(max(0, log.tell() - TAIL_BYTES))
        output = log.read().decode("utf-8", errors="replace")
    return {
        "argv": argv, "exit_code": code, "output_tail": output,
        "duration_s": round(monotonic() - started, 3), "timed_out": timed_out,
    }


def _recipes(package: Path) -> dict[str, tuple[Path, dict]]:
    rows = {}
    for path in sorted((package / "tests").glob("*/recipe.json")):
        recipe = json.loads(path.read_text(encoding="utf-8"))
        rows[recipe["id"]] = (path, recipe)
    return rows


def _load_files(recipe_path: Path, recipe: dict) -> list[tuple[str, bytes]]:
    files = []
    root = (recipe_path.parent / "files").resolve()
    for entry in recipe["files"]:
        source = (root / entry["path"]).resolve()
        if not source.is_relative_to(root):
            raise ValueError(f"test source path escapes the package: {entry['path']}")
        content = source.read_bytes()
        if sha256(content).hexdigest() != entry["sha256"]:
            raise ValueError(f"test source hash mismatch: {source}")
        files.append((entry["path"], content))
    return files


def replay(package: Path, repo: Path, selected: list[str], output: Path,
           timeout_s: float) -> dict:
    recipes = _recipes(package)
    unknown = set(selected) - recipes.keys()
    if unknown:
        raise ValueError(f"unknown finding IDs: {', '.join(sorted(unknown))}")
    chosen = selected or sorted(recipes)
    if not chosen:
        raise ValueError("no packaged test recipes found")
    if output.exists():
        raise ValueError(f"output already exists: {output}")
    output.mkdir(parents=True)
    results = []
    for identity in chosen:
        recipe_path, recipe = recipes[identity]
        log_path = output / f"{identity}.log"
        with TemporaryDirectory(prefix=f"{identity.lower()}-") as temporary:
            workspace = Path(temporary) / "source"
            _clone_project(repo, workspace)
            errors = _place_files(workspace, _load_files(recipe_path, recipe))
            commands, status = [], "reproduced"
            if errors:
                status = "source_error"
            for argv in recipe["build"] if not errors else []:
                outcome = _run_command(argv, workspace, timeout_s)
                commands.append(outcome)
                if outcome["exit_code"] != 0:
                    status = "build_error"
                    break
            if status == "reproduced":
                outcome = _run_command(recipe["test"], workspace, timeout_s)
                commands.append(outcome)
                marker = recipe["expected"].get("output_contains", "")
                if outcome["timed_out"] or outcome["exit_code"] is None:
                    status = "error"
                elif outcome["exit_code"] == 0:
                    status = "unexpected_pass"
                elif outcome["exit_code"] < 0:
                    status = "error"
                elif marker and marker not in outcome["output_tail"]:
                    status = "wrong_failure"
            log_path.write_text("\n\n".join(
                " ".join(command["argv"]) + "\n" + command["output_tail"]
                for command in commands), encoding="utf-8")
            results.append({
                "id": identity, "status": status, "errors": errors,
                "commands": commands, "log": log_path.name,
            })
    report = {
        "schema": "rfc-deviation-replay/1",
        "repo": str(repo.resolve()),
        "results": results,
        "reproduced": sum(row["status"] == "reproduced" for row in results),
        "total": len(results),
    }
    (output / "report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return report


def main(argv: list[str] | None = None, *, package: Path | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd(),
                        help="Project checkout to copy and test (default: current directory)")
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--finding", action="append", default=[],
                           help="Finding ID to replay; repeat for multiple")
    selection.add_argument("--all", action="store_true",
                           help="Replay every packaged finding (default when none selected)")
    parser.add_argument("--output", type=Path, default=Path("rfc-deviation-results"))
    parser.add_argument("--timeout", type=float, default=600.0)
    args = parser.parse_args(argv)
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    package = (package or Path(__file__).resolve().parent).resolve()
    report = replay(package, args.repo.resolve(), args.finding, args.output.resolve(), args.timeout)
    print(json.dumps(report, indent=2))
    return 0 if report["reproduced"] == report["total"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
