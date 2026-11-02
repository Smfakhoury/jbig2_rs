import copy
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import io
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from urllib.error import HTTPError


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

import audit_package
import check_release as release


COMMIT = "a" * 40
HEAD = "b" * 40
CHECKSUM = "c" * 64
MANIFEST = (
    '[package]\nname = "jbig2-decoder"\nversion = "0.1.1"\nrust-version = "1.88.0"\n'
    'publish = ["crates-io"]\n[lib]\nname = "jbig2_decoder"\n'
)


def check_metadata(commit=HEAD, phase="pull_request"):
    checks, runs = [], {}
    for index, (name, workflow) in enumerate(release.REQUIRED_CHECKS.items(), 1):
        if workflow is None and phase == "integrated":
            continue
        run_id = str(index)
        checks.append({
            "id": index, "name": name, "head_sha": commit,
            "status": "completed", "conclusion": "success",
            "app": {"id": 15368 if workflow else 95686,
                    "slug": "github-actions" if workflow else "microsoft-github-policy-service"},
            "details_url": f"https://github.com/{release.REPOSITORY}/actions/runs/{run_id}/job/1",
            "check_suite": {"id": index},
        })
        if workflow:
            runs[run_id] = {
                "repository": {"full_name": release.REPOSITORY}, "head_sha": commit,
                "path": f".github/workflows/{workflow}.yml",
                "event": "push" if phase == "integrated" else "pull_request",
                "status": "completed", "conclusion": "success", "check_suite_id": index,
            }
    return checks, runs


def api_fixture(checks, runs):
    def api(endpoint):
        if "/check-runs?" in endpoint:
            return {"check_runs": checks}
        return runs[endpoint.rsplit("/", 1)[1]]
    return api


class ReleaseChecksTests(unittest.TestCase):
    def test_exact_required_checks_and_integration_checks_pass(self):
        for phase, count in (("pull_request", 30), ("integrated", 29)):
            checks, runs = check_metadata(phase=phase)
            report = release.validate_checks(HEAD, phase, api_fixture(checks, runs))
            self.assertEqual(len(report["check_ids"]), count)

    def test_missing_failed_pending_wrong_commit_or_spoofed_checks_fail(self):
        for phase in ("pull_request", "integrated"):
            for mutation in ("missing", "failed", "pending", "commit", "app", "url",
                             "repository", "run_commit", "path", "event", "run_failed", "suite"):
                checks, runs = check_metadata(phase=phase)
                check, run = checks[0], runs["1"]
                if mutation == "missing":
                    checks.pop(0)
                elif mutation == "failed":
                    check["conclusion"] = "failure"
                elif mutation == "pending":
                    check["status"] = "in_progress"
                elif mutation == "commit":
                    check["head_sha"] = COMMIT
                elif mutation == "app":
                    check["app"]["id"] = 123
                elif mutation == "url":
                    check["details_url"] = check["details_url"].replace(release.REPOSITORY, "fork/repo")
                elif mutation == "repository":
                    run["repository"]["full_name"] = "fork/repo"
                elif mutation == "run_commit":
                    run["head_sha"] = COMMIT
                elif mutation == "path":
                    run["path"] = ".github/workflows/impostor.yml"
                elif mutation == "event":
                    run["event"] = "workflow_dispatch"
                elif mutation == "run_failed":
                    run["conclusion"] = "failure"
                elif mutation == "suite":
                    run["check_suite_id"] = 999
                with self.subTest(phase=phase, mutation=mutation), self.assertRaises(ValueError):
                    release.validate_checks(HEAD, phase, api_fixture(checks, runs))

    def test_latest_failed_rerun_cannot_reuse_an_older_pass(self):
        checks, runs = check_metadata()
        changed = {**checks[0], "id": 999, "conclusion": "failure"}
        with self.assertRaises(ValueError):
            release.validate_checks(HEAD, "pull_request", api_fixture(checks + [changed], runs))

    def test_cla_issuer_is_required(self):
        checks, runs = check_metadata()
        next(check for check in checks if check["name"] == "license/cla")["app"]["id"] = 15368
        with self.assertRaisesRegex(ValueError, "CLA"):
            release.validate_checks(HEAD, "pull_request", api_fixture(checks, runs))

    def test_unknown_phase_and_unbounded_pagination_fail(self):
        with self.assertRaises(ValueError):
            release.validate_checks(HEAD, "unknown")
        with self.assertRaisesRegex(ValueError, "bounded"):
            release.pages("fixture", api=lambda endpoint: [1] * 100)
        calls = []
        def api(endpoint):
            calls.append(endpoint)
            return list(range(100)) if endpoint.endswith("&page=1") else [100]
        self.assertEqual(len(release.pages("fixture", api=api)), 101)
        self.assertEqual(len(calls), 2)

    def test_invalid_or_shell_shaped_tags_are_rejected_before_git(self):
        for tag in ("0.1.1", "main", "v01.1.1", "v0.1.1-01", "v0.1.1-rc.",
                    "v0.1.1+build", "v0.1.1; echo injected", "$(echo injected)", "v0.1.1\n"):
            with self.subTest(tag=tag), patch.object(release, "git") as git:
                with self.assertRaises(ValueError):
                    release.validate_tag(Path("."), tag)
                git.assert_not_called()
        self.assertIsNotNone(release.TAG.fullmatch("v0.1.2-rc.1"))

    def test_exact_merged_pr_tree_and_tag_policy(self):
        pr = {
            "number": 1, "merged": True, "merged_at": "2026-11-02",
            "merge_commit_sha": COMMIT, "head": {"sha": HEAD},
            "base": {"ref": "main", "repo": {"full_name": release.REPOSITORY}},
        }
        def api(endpoint):
            if "/git/commits/" in endpoint:
                return {"sha": HEAD, "tree": {"sha": "d" * 40}}
            return [pr] if "/pulls?" in endpoint else pr
        def git(root, *args):
            if args[0] == "show":
                return MANIFEST
            if args[-1].endswith("^{tree}"):
                return "d" * 40
            return COMMIT
        with (
            patch.object(release, "git", side_effect=git),
            patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)),
            patch.object(release, "validate_checks", return_value={"checked": True}) as checks,
        ):
            self.assertEqual(release.validate_tag(Path("."), "v0.1.1", api)["commit"], COMMIT)
            self.assertEqual(checks.call_count, 2)
            for mutation in ("unmerged", "other_base", "different_merge"):
                changed = copy.deepcopy(pr)
                if mutation == "unmerged":
                    changed["merged_at"] = None
                elif mutation == "other_base":
                    changed["base"]["ref"] = "dev"
                else:
                    changed["merge_commit_sha"] = HEAD
                with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                    release.validate_tag(Path("."), "v0.1.1", lambda endpoint: [changed])
            with patch.object(release, "git", side_effect=[COMMIT, MANIFEST, HEAD]):
                with self.assertRaisesRegex(ValueError, "tree differs"):
                    release.validate_tag(Path("."), "v0.1.1", api)
            with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)):
                with self.assertRaisesRegex(ValueError, "reachable"):
                    release.validate_tag(Path("."), "v0.1.1", api)

    def test_squash_merge_works_without_the_original_head_in_a_fresh_clone(self):
        with tempfile.TemporaryDirectory(prefix="jbig2-release-pr-") as temporary:
            directory = Path(temporary)
            source, clone = directory / "source", directory / "clone"
            source.mkdir()
            release.git(source, "init", "-b", "main")
            release.git(source, "config", "user.name", "Release Test")
            release.git(source, "config", "user.email", "release@example.invalid")
            (source / "Cargo.toml").write_text(MANIFEST)
            (source / "state.txt").write_text("base")
            release.git(source, "add", ".")
            trailer = "Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>"
            release.git(source, "commit", "-m", "Base", "-m", trailer)
            release.git(source, "checkout", "-b", "topic")
            (source / "state.txt").write_text("reviewed")
            release.git(source, "add", ".")
            release.git(source, "commit", "-m", "Reviewed head", "-m", trailer)
            head = release.git(source, "rev-parse", "HEAD")
            tree = release.git(source, "rev-parse", "HEAD^{tree}")
            release.git(source, "checkout", "main")
            release.git(source, "merge", "--squash", "topic")
            release.git(source, "commit", "-m", "Integrated release", "-m", trailer)
            commit = release.git(source, "rev-parse", "HEAD")
            release.git(source, "tag", "v0.1.1")
            subprocess.run(
                ["git", "clone", "--no-local", "--single-branch", "--branch", "main", str(source), str(clone)],
                check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            absent = subprocess.run(
                ["git", "-C", str(clone), "cat-file", "-e", head],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            )
            self.assertNotEqual(absent.returncode, 0)
            pr = {"number": 1, "merged": True, "merged_at": "2026-11-02", "merge_commit_sha": commit,
                  "head": {"sha": head}, "base": {"ref": "main", "repo": {"full_name": release.REPOSITORY}}}
            def api(endpoint):
                if "/git/commits/" in endpoint:
                    return {"sha": head, "tree": {"sha": tree}}
                return [pr] if "/pulls?" in endpoint else pr
            with patch.object(release, "validate_checks", return_value={"checked": True}):
                self.assertEqual(release.validate_tag(clone, "v0.1.1", api)["commit"], commit)

    def test_publication_target_version_and_msrv_are_not_relaxed(self):
        import tomllib
        original = tomllib.loads(MANIFEST)
        release.check_manifest(original, "0.1.1")
        for key, value in (("publish", False), ("publish", None), ("publish", ["other"]),
                           ("version", "0.1.2"), ("rust-version", "1.89.0"), ("name", "other")):
            document = copy.deepcopy(original)
            document["package"][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                release.check_manifest(document, "0.1.1")


class ReleaseArchiveTests(unittest.TestCase):
    def test_archive_source_manifest_vcs_and_inventory_are_exact(self):
        files = {"Cargo.toml": MANIFEST.encode(), "README.md": b"readme", "LICENSE": b"license",
                 "SECURITY.md": b"security", "src/lib.rs": b"pub fn decoder() {}"}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, data in files.items():
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_bytes(data)
            original = {**files, "Cargo.toml.orig": files["Cargo.toml"],
                        ".cargo_vcs_info.json": json.dumps({"git": {"sha1": COMMIT, "dirty": False}}).encode()}
            for mutation in ("none", "source", "original_manifest", "manifest", "vcs", "dirty_vcs",
                             "missing_vcs", "unexpected", "symlink"):
                entries = dict(original)
                if mutation == "source":
                    entries["src/lib.rs"] = b"different"
                elif mutation == "original_manifest":
                    entries["Cargo.toml.orig"] = b"different"
                elif mutation == "manifest":
                    entries["Cargo.toml"] = MANIFEST.replace("0.1.1", "0.1.2").encode()
                elif mutation in ("vcs", "dirty_vcs"):
                    entries[".cargo_vcs_info.json"] = json.dumps({
                        "git": {"sha1": HEAD if mutation == "vcs" else COMMIT,
                                "dirty": mutation == "dirty_vcs"},
                    }).encode()
                elif mutation == "missing_vcs":
                    entries.pop(".cargo_vcs_info.json")
                elif mutation == "unexpected":
                    entries["proofs/secret.rs"] = b"not a runtime asset"
                archive = root / "fixture.crate"
                with tarfile.open(archive, "w:gz") as tar:
                    for name, data in entries.items():
                        member = tarfile.TarInfo(f"jbig2-decoder-0.1.1/{name}")
                        member.size = len(data)
                        if mutation == "symlink" and name == "src/lib.rs":
                            member.type = tarfile.SYMTYPE
                            member.linkname = "/outside"
                            member.size = 0
                            tar.addfile(member)
                        else:
                            tar.addfile(member, io.BytesIO(data))
                with (
                    self.subTest(mutation=mutation),
                    patch.object(audit_package, "REQUIRED", set(files)),
                    patch.object(release, "git", side_effect=[COMMIT, ""]),
                ):
                    if mutation == "none":
                        self.assertEqual(release.verify_archive(root, archive, COMMIT, "0.1.1"),
                                         hashlib.sha256(archive.read_bytes()).hexdigest())
                    else:
                        with self.assertRaises((ValueError, FileNotFoundError)):
                            release.verify_archive(root, archive, COMMIT, "0.1.1")

    def test_dirty_checkout_or_wrong_commit_fails_before_archive_access(self):
        for values in ([HEAD], [COMMIT, " M src/lib.rs"]):
            with patch.object(release, "git", side_effect=values), self.assertRaises(ValueError):
                release.verify_archive(Path("."), Path("missing"), COMMIT, "0.1.1")


class ReleaseRegistryTests(unittest.TestCase):
    def test_only_genuine_missing_versions_allow_publication(self):
        for status in (404, 403, 429, 500):
            error = HTTPError("https://crates.io", status, "fixture", {}, None)
            with self.subTest(status=status), patch.object(release, "request", side_effect=error):
                if status == 404:
                    self.assertTrue(release.registry_state("0.1.1", CHECKSUM)["publish_needed"])
                else:
                    with self.assertRaises(HTTPError):
                        release.registry_state("0.1.1", CHECKSUM)

    def test_existing_version_is_skipped_only_with_matching_unyanked_identity(self):
        version = {"crate": "jbig2-decoder", "num": "0.1.1", "checksum": CHECKSUM, "yanked": False}
        with patch.object(release, "request", return_value=json.dumps({"version": version}).encode()):
            self.assertFalse(release.registry_state("0.1.1", CHECKSUM)["publish_needed"])
        for key, value in (("checksum", "d" * 64), ("yanked", True),
                           ("yanked", None), ("crate", "other"), ("num", "0.1.2")):
            changed = {**version, key: value}
            with (
                self.subTest(key=key),
                patch.object(release, "request", return_value=json.dumps({"version": changed}).encode()),
                self.assertRaises(ValueError),
            ):
                release.registry_state("0.1.1", CHECKSUM)

    def test_post_upload_visibility_has_a_bounded_retry_without_success_fallback(self):
        error = HTTPError("https://crates.io", 404, "fixture", {}, None)
        with (
            patch.object(release, "request", side_effect=error),
            patch.object(release.time, "monotonic", side_effect=[0, 1, 91]),
            patch.object(release.time, "sleep") as sleep,
            self.assertRaisesRegex(ValueError, "not visible"),
        ):
            release.registry_state("0.1.1", CHECKSUM, require=True)
        sleep.assert_called_once_with(3)

    def test_registry_consumer_reuses_exact_output_probe_and_checks_source_and_lock(self):
        graph = {"packages": [
            {"name": "registry-consumer", "version": "0.0.0", "source": None},
            {"name": "jbig2-decoder", "version": "0.1.1", "dependencies": [],
             "source": "registry+https://github.com/rust-lang/crates.io-index"},
        ]}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "tools").mkdir()
            (root / "tools/distribution_consumer.rs").write_text("fn main() {}")
            for mutation in ("none", "graph", "version", "path", "checksum"):
                changed = copy.deepcopy(graph)
                if mutation == "graph":
                    changed["packages"].append({"name": "unexpected"})
                elif mutation == "version":
                    changed["packages"][1]["version"] = "0.1.2"
                elif mutation == "path":
                    changed["packages"][1]["source"] = None
                def run(command, directory, environment):
                    if "generate-lockfile" in command:
                        checksum = "d" * 64 if mutation == "checksum" else CHECKSUM
                        (directory / "Cargo.lock").write_text(
                            '[[package]]\nname="jbig2-decoder"\nversion="0.1.1"\n'
                            'source="registry+https://github.com/rust-lang/crates.io-index"\n'
                            f'checksum="{checksum}"\n',
                        )
                with (
                    self.subTest(mutation=mutation),
                    patch("check_distribution.run", side_effect=run) as commands,
                    patch("check_distribution.metadata", return_value=changed),
                    patch("check_distribution.require_rejected_check") as rejected,
                ):
                    if mutation == "none":
                        report = release.verify_registry_consumer(root, "0.1.1", CHECKSUM)
                        self.assertEqual(report["registry_normal_dependencies"], [])
                        self.assertEqual(commands.call_args.args[0], ["cargo", "+1.88.0", "run", "--offline", "--locked"])
                        self.assertEqual(commands.call_args.args[2]["CARGO_NET_OFFLINE"], "true")
                        self.assertEqual(rejected.call_args.args[-2:], ("E0432", "runtime_access"))
                    else:
                        with self.assertRaises(ValueError):
                            release.verify_registry_consumer(root, "0.1.1", CHECKSUM)
                        rejected.assert_not_called()


class ReleaseCommandTests(unittest.TestCase):
    def invoke(self, output, *extra):
        with (
            patch.object(sys, "argv", ["check_release.py", "--tag", "v0.1.1", "--output", str(output), *extra]),
            redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()),
        ):
            return release.main()

    def test_moved_tags_wrong_checksum_and_missing_checksum_leave_no_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "report.json"
            arguments = ["--commit", COMMIT, "--archive", "fixture.crate", "--expected-sha256", CHECKSUM]
            for mutation in ("tag", "checksum", "empty_checksum"):
                output.write_text('{"status":"old"}')
                modified = list(arguments)
                if mutation == "empty_checksum":
                    modified[-1] = ""
                with (
                    self.subTest(mutation=mutation),
                    patch.object(release, "git", return_value=HEAD if mutation == "tag" else COMMIT),
                    patch.object(release, "verify_archive", return_value="d" * 64),
                    patch.object(release, "registry_state") as registry,
                ):
                    self.assertEqual(self.invoke(output, *modified), 1)
                    self.assertFalse(output.exists())
                    registry.assert_not_called()

    def test_post_publication_download_is_checked_before_consumer(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "report.json"
            data = b"actual registry archive fixture"
            checksum = hashlib.sha256(data).hexdigest()
            arguments = ["--commit", COMMIT, "--archive", "fixture.crate", "--expected-sha256", checksum,
                         "--verify-published"]
            for valid in (True, False):
                with (
                    self.subTest(valid=valid),
                    patch.object(release, "git", return_value=COMMIT),
                    patch.object(release, "verify_archive", return_value=checksum),
                    patch.object(release, "registry_state", return_value={"publish_needed": False}),
                    patch.object(release, "request", return_value=data if valid else b"wrong"),
                    patch.object(release, "verify_registry_consumer", return_value={"consumer": "verified"}) as consumer,
                ):
                    self.assertEqual(self.invoke(output, *arguments), 0 if valid else 1)
                    if valid:
                        self.assertTrue(json.loads(output.read_text())["registry_download_verified"])
                        consumer.assert_called_once()
                    else:
                        self.assertFalse(output.exists())
                        consumer.assert_not_called()
