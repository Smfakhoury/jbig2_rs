#!/usr/bin/env python3
"""Validate release provenance and registry identity without publishing or credentials."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import tomllib
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen


REPOSITORY = "microsoft/jbig2_rs"
PACKAGE = "jbig2-decoder"
TAG = re.compile(
    r"v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)"
    r"(?:\.(?:0|[1-9][0-9]*|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?"
)
SHA = re.compile(r"[0-9a-f]{40}")
CHECKSUM = re.compile(r"[0-9a-f]{64}")
REQUIRED_CHECKS = {
    **{f"cargo ({os})": "ci" for os in ("ubuntu-latest", "windows-latest", "macos-latest")},
    **{f"distribution ({os}, {toolchain})": "ci"
       for os in ("ubuntu-latest", "windows-latest", "macos-latest")
       for toolchain in ("1.88.0", "stable")},
    "coverage": "coverage", "controls": "fuzz", "fuzz": "fuzz",
    "performance": "performance", "whole": "verus", "verify": "verus",
    **{f"semantic ({index})": "verus" for index in range(14)},
    "license/cla": None,
}


def git(root, *arguments):
    return subprocess.run(
        ["git", "-C", str(root), *arguments], check=True, text=True, stdout=subprocess.PIPE,
    ).stdout.strip()


def github(endpoint):
    return json.loads(subprocess.run(
        ["gh", "api", endpoint], check=True, text=True, stdout=subprocess.PIPE,
    ).stdout)


def pages(endpoint, field=None, api=github):
    entries = []
    for page in range(1, 11):
        result = api(f"{endpoint}?per_page=100&page={page}")
        batch = result[field] if field else result
        if not isinstance(batch, list):
            raise ValueError("invalid GitHub pagination response")
        entries.extend(batch)
        if len(batch) < 100:
            return entries
    raise ValueError("release metadata exceeds the bounded GitHub pagination limit")


def check_manifest(document, version):
    from check_distribution import crates_io_publication_enabled

    package = document["package"]
    if (
        package["name"] != PACKAGE or package["version"] != version
        or package.get("rust-version") != "1.88.0"
        or not crates_io_publication_enabled(package)
    ):
        raise ValueError("release tag/version, MSRV or crates.io-only target mismatch")


def validate_checks(commit, phase, api=github):
    if phase not in ("pull_request", "integrated"):
        raise ValueError("unknown release validation phase")
    entries = pages(f"repos/{REPOSITORY}/commits/{commit}/check-runs", "check_runs", api)
    latest = {}
    for entry in entries:
        if entry["name"] in REQUIRED_CHECKS:
            name = entry["name"]
            if name not in latest or entry["id"] > latest[name]["id"]:
                latest[name] = entry
    expected = set(REQUIRED_CHECKS) - ({"license/cla"} if phase == "integrated" else set())
    missing = expected - set(latest)
    if missing:
        raise ValueError(f"{phase}: missing required checks: {sorted(missing)}")
    runs = {}
    for name in sorted(expected):
        check = latest[name]
        if (
            check["head_sha"] != commit or check["status"] != "completed"
            or check["conclusion"] != "success"
        ):
            raise ValueError(f"{phase}: required check did not pass: {name}")
        workflow = REQUIRED_CHECKS[name]
        if workflow is None:
            if check["app"]["id"] != 95686 or check["app"]["slug"] != "microsoft-github-policy-service":
                raise ValueError("CLA check has an unexpected issuer")
            continue
        if check["app"]["id"] != 15368 or check["app"]["slug"] != "github-actions":
            raise ValueError(f"unexpected check issuer: {name}")
        match = re.fullmatch(
            rf"https://github\.com/{REPOSITORY}/actions/runs/([0-9]+)/job/[0-9]+",
            check["details_url"],
        )
        if not match:
            raise ValueError(f"unexpected workflow run URL: {name}")
        run_id = match[1]
        if run_id not in runs:
            runs[run_id] = api(f"repos/{REPOSITORY}/actions/runs/{run_id}")
        run = runs[run_id]
        # PR check runs identify the PR head, not necessarily the checkout's merge commit.
        if (
            run["repository"]["full_name"] != REPOSITORY
            or run["head_sha"] != commit
            or run["path"] != f".github/workflows/{workflow}.yml"
        ):
            raise ValueError(f"unexpected source workflow: {name}")
        if (
            run["status"] != "completed" or run["conclusion"] != "success"
            or run["check_suite_id"] != check["check_suite"]["id"]
            or (phase == "pull_request" and run["event"] != "pull_request")
            or (phase == "integrated" and run["event"] != "push")
        ):
            raise ValueError(f"workflow run did not validate the required source: {name}")
    return {"commit": commit, "check_ids": [latest[name]["id"] for name in sorted(expected)],
            "run_ids": sorted(runs)}


def validate_tag(root, tag, api=github):
    if not TAG.fullmatch(tag):
        raise ValueError("tag must be a vMAJOR.MINOR.PATCH Cargo version")
    commit = git(root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    if not SHA.fullmatch(commit):
        raise ValueError("tag did not resolve to a GitHub commit")
    ancestor = subprocess.run(
        ["git", "-C", str(root), "merge-base", "--is-ancestor", commit, "origin/main"],
        check=False,
    )
    if ancestor.returncode == 1:
        raise ValueError("release source is not reachable from main")
    ancestor.check_returncode()
    check_manifest(tomllib.loads(git(root, "show", f"{commit}:Cargo.toml")), tag[1:])
    associated = pages(f"repos/{REPOSITORY}/commits/{commit}/pulls", api=api)
    matching = [pr for pr in associated if (
        pr["merge_commit_sha"] == commit and pr["merged_at"]
        and pr["base"]["ref"] == "main" and pr["base"]["repo"]["full_name"] == REPOSITORY
    )]
    if len(matching) != 1:
        raise ValueError("release source must be the exact integration commit of one merged main PR")
    pr = api(f"repos/{REPOSITORY}/pulls/{matching[0]['number']}")
    if not pr["merged"] or pr["merge_commit_sha"] != commit:
        raise ValueError("release PR integration identity changed")
    head = pr["head"]["sha"]
    if not SHA.fullmatch(head):
        raise ValueError("invalid reviewed PR head identity")
    head_commit = api(f"repos/{REPOSITORY}/git/commits/{head}")
    if (
        head_commit["sha"] != head
        or head_commit["tree"]["sha"] != git(root, "rev-parse", f"{commit}^{{tree}}")
    ):
        raise ValueError("release tree differs from the reviewed PR head; update/retest the PR")
    return {
        "tag": tag, "version": tag[1:], "commit": commit, "pull_request": pr["number"],
        "pull_request_checks": validate_checks(head, "pull_request", api),
        "integrated_source_checks": validate_checks(commit, "integrated", api),
    }


def verify_archive(root, archive, commit, version):
    from audit_package import REQUIRED
    from check_distribution import extract_archive

    if not SHA.fullmatch(commit) or not TAG.fullmatch("v" + version):
        raise ValueError("invalid immutable source/version")
    if git(root, "rev-parse", "HEAD") != commit or git(root, "status", "--porcelain"):
        raise ValueError("publication requires a clean checkout of the validated commit")
    check_manifest(tomllib.loads((root / "Cargo.toml").read_text()), version)
    with tarfile.open(archive, "r:gz") as source:
        members = source.getmembers()
        if len(members) > 4096 or sum(member.size for member in members) > 64 * 1024 * 1024:
            raise ValueError("release archive exceeds the bounded inventory/size limit")
    with tempfile.TemporaryDirectory(prefix="jbig2-release-archive-") as directory:
        destination = Path(directory)
        extract_archive(archive, destination, f"{PACKAGE}-{version}")
        extracted = destination / f"{PACKAGE}-{version}"
        for name in REQUIRED - {"Cargo.toml"}:
            if (extracted / name).read_bytes() != (root / name).read_bytes():
                raise ValueError(f"archive source differs: {name}")
        if (extracted / "Cargo.toml.orig").read_bytes() != (root / "Cargo.toml").read_bytes():
            raise ValueError("archive original manifest differs from the validated source")
        check_manifest(tomllib.loads((extracted / "Cargo.toml").read_text()), version)
        vcs = json.loads((extracted / ".cargo_vcs_info.json").read_bytes())
        if vcs["git"]["sha1"] != commit or vcs["git"].get("dirty", False) is not False:
            raise ValueError("archive VCS identity differs from the validated source")
    return hashlib.sha256(archive.read_bytes()).hexdigest()


def request(url, limit=2 * 1024 * 1024):
    with urlopen(Request(url, headers={
        "User-Agent": f"{PACKAGE}-release-validation (https://github.com/{REPOSITORY})",
    }), timeout=30) as response:
        data = response.read(limit + 1)
        if len(data) > limit:
            raise ValueError("registry response exceeds the bounded size limit")
        return data


def registry_state(version, checksum, require=False):
    if not TAG.fullmatch("v" + version) or not CHECKSUM.fullmatch(checksum):
        raise ValueError("invalid registry version/checksum")
    deadline = time.monotonic() + 90
    while True:
        try:
            document = json.loads(request(f"https://crates.io/api/v1/crates/{PACKAGE}/{version}"))
            break
        except HTTPError as error:
            if error.code != 404:
                raise
            if not require:
                return {"registry_status": "not_published", "publish_needed": True}
            if time.monotonic() >= deadline:
                raise ValueError("uploaded version is not visible in the registry") from error
            time.sleep(3)
    published = document["version"]
    if (
        published["crate"] != PACKAGE or published["num"] != version
        or published["checksum"] != checksum or published["yanked"] is not False
    ):
        raise ValueError("existing registry version is yanked or has a different package identity")
    return {"registry_status": "verified_existing", "publish_needed": False,
            "registry_checksum": published["checksum"]}


def verify_registry_consumer(root, version, checksum):
    from check_distribution import isolated_environment, metadata, require_rejected_check, run

    with tempfile.TemporaryDirectory(prefix="jbig2-registry-consumer-") as temporary:
        directory = Path(temporary)
        consumer = directory / "consumer"
        (consumer / "src").mkdir(parents=True)
        (consumer / "Cargo.toml").write_text(
            '[package]\nname = "registry-consumer"\nversion = "0.0.0"\nedition = "2021"\n'
            f'[workspace]\n[dependencies]\n{PACKAGE} = "={version}"\n',
        )
        main = consumer / "src/main.rs"
        main.write_bytes((root / "tools/distribution_consumer.rs").read_bytes())
        environment = isolated_environment(directory)
        online = {**environment, "CARGO_NET_OFFLINE": "false"}
        cargo = ["cargo", "+1.88.0"]
        run(cargo + ["generate-lockfile"], consumer, online)
        run(cargo + ["fetch", "--locked"], consumer, online)
        graph = metadata(cargo, consumer, environment)
        packages = {package["name"]: package for package in graph["packages"]}
        if len(graph["packages"]) != 2 or set(packages) != {PACKAGE, "registry-consumer"}:
            raise ValueError("unexpected registry consumer dependency graph")
        decoder = packages[PACKAGE]
        locked = tomllib.loads((consumer / "Cargo.lock").read_text())
        lock_decoders = [package for package in locked["package"] if package["name"] == PACKAGE]
        if len(lock_decoders) != 1:
            raise ValueError("registry consumer lockfile lacks one exact decoder entry")
        lock_decoder = lock_decoders[0]
        if (
            decoder["version"] != version or decoder["dependencies"]
            or decoder["source"] != "registry+https://github.com/rust-lang/crates.io-index"
            or lock_decoder["version"] != version
            or lock_decoder["source"] != decoder["source"]
            or lock_decoder["checksum"] != checksum
        ):
            raise ValueError("registry consumer did not resolve the exact checked archive")
        run(cargo + ["run", "--offline", "--locked"], consumer, environment)
        main.write_text("use jbig2_decoder::runtime_access;\nfn main() {}\n")
        require_rejected_check(cargo, consumer, environment, "E0432", "runtime_access")
        return {"registry_consumer": "fresh_exact_version_checksum_offline_output_verified",
                "registry_normal_dependencies": [], "registry_runtime_access": "not_exported"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit")
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--expected-sha256")
    parser.add_argument("--verify-published", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--github-output", type=Path)
    arguments = parser.parse_args()
    try:
        arguments.output.unlink(missing_ok=True)
        sys.path.insert(0, str(arguments.root / "tools"))
        if arguments.expected_sha256 is not None and not CHECKSUM.fullmatch(arguments.expected_sha256):
            raise ValueError("expected publication checksum must be a SHA-256 digest")
        if arguments.archive:
            if not arguments.commit or not TAG.fullmatch(arguments.tag):
                raise ValueError("archive verification requires a validated commit and version tag")
            version = arguments.tag[1:]
            if git(arguments.root, "rev-parse", f"refs/tags/{arguments.tag}^{{commit}}") != arguments.commit:
                raise ValueError("release tag moved after source validation")
            checksum = verify_archive(arguments.root, arguments.archive, arguments.commit, version)
            if arguments.expected_sha256 is not None and checksum != arguments.expected_sha256:
                raise ValueError("publication archive differs from the validation job archive")
            report = {"tag": arguments.tag, "version": version, "commit": arguments.commit,
                      "archive_sha256": checksum, **registry_state(version, checksum, arguments.verify_published)}
            if arguments.verify_published:
                data = request(
                    f"https://static.crates.io/crates/{PACKAGE}/{PACKAGE}-{version}.crate",
                    limit=32 * 1024 * 1024,
                )
                if hashlib.sha256(data).hexdigest() != checksum:
                    raise ValueError("downloaded registry archive checksum mismatch")
                report["registry_download_verified"] = True
                report.update(verify_registry_consumer(arguments.root, version, checksum))
        else:
            if arguments.commit or arguments.expected_sha256 or arguments.verify_published:
                raise ValueError("archive verification options require --archive")
            report = validate_tag(arguments.root, arguments.tag)
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(json.dumps(report, indent=2) + "\n")
        if arguments.github_output:
            with arguments.github_output.open("a") as output:
                for key in ("commit", "version", "archive_sha256", "publish_needed"):
                    if key in report:
                        value = str(report[key]).lower() if isinstance(report[key], bool) else report[key]
                        output.write(f"{key}={value}\n")
        print(json.dumps(report))
        return 0
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"release validation failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
