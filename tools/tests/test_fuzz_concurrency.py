import copy
import re
import sys
import unittest
from pathlib import Path


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from check_workflows import check_workflow, load_workflow


class FuzzProducerConcurrencyTests(unittest.TestCase):
    def setUp(self):
        self.workflow = load_workflow((TOOLS.parent / ".github/workflows/fuzz.yml").read_text())
        self.job = self.workflow["jobs"]["fuzz"]
        self.trust = (
            "(github.event_name == 'schedule' || github.event_name == 'workflow_dispatch')"
            " && github.ref == 'refs/heads/main' && github.event.repository.default_branch == 'main'"
        )

    def group(self, event="workflow_dispatch", ref="refs/heads/main", default="main",
              run_id=1, attempt=1, repository="owner/project", sha="a" * 40, profile="smoke"):
        expression = self.job["concurrency"]["group"]
        self.assertTrue(expression.startswith("${{ " + self.trust + " && format("))
        formats = re.findall(r"format\('([^']+)', ([^)]+)\)", expression)
        self.assertEqual(len(formats), 2)
        trusted = event in {"schedule", "workflow_dispatch"} and ref == "refs/heads/main" and default == "main"
        template, arguments = formats[0 if trusted else 1]
        context = {
            "github.repository": repository, "github.run_id": run_id,
            "github.run_attempt": attempt, "github.sha": sha, "inputs.profile": profile,
        }
        return template.format(*(context[argument] for argument in arguments.split(", ")))

    def test_exact_trust_job_scope_and_permitted_contexts(self):
        self.assertEqual(self.job["env"]["TRUSTED_CORPUS"], "${{ " + self.trust + " }}")
        self.assertEqual(self.job["concurrency"]["cancel-in-progress"], "false")
        expression = self.job["concurrency"]["group"]
        roots = re.findall(r"(?<![.\w])([a-z_][a-z_0-9]*)\.", expression)
        self.assertEqual(set(roots), {"github"})
        self.assertNotIn("concurrency", self.workflow)
        self.assertNotIn("concurrency", self.workflow["jobs"]["controls"])
        self.assertTrue(check_workflow(self.workflow, "fuzz.yml"))

    def test_same_source_manual_and_schedule_producers_share_one_main_group(self):
        expected = "fuzz-generated-v2-trusted-owner/project-main"
        for event in ("workflow_dispatch", "schedule"):
            for run_id, attempt, profile in ((101, 1, "smoke"), (102, 1, "extended"), (101, 2, "smoke")):
                with self.subTest(event=event, run_id=run_id, attempt=attempt):
                    self.assertEqual(self.group(event=event, run_id=run_id, attempt=attempt, profile=profile), expected)
        self.assertEqual(self.group(sha="a" * 40), self.group(sha="b" * 40, run_id=999))
        self.assertNotEqual(self.group(repository="other/project"), expected)

    def test_all_untrusted_runs_and_attempts_are_isolated_from_trusted_and_each_other(self):
        trusted = self.group()
        untrusted = [
            {"event": "pull_request", "ref": "refs/pull/7/merge"},
            {"event": "pull_request", "ref": "refs/heads/main"},
            {"event": "push"},
            {"event": "workflow_dispatch", "ref": "refs/heads/topic"},
            {"event": "workflow_dispatch", "default": "develop"},
            {"event": "schedule", "ref": "refs/heads/topic"},
        ]
        groups = []
        for index, context in enumerate(untrusted):
            first = self.group(**context, run_id=100 + index)
            rerun = self.group(**context, run_id=100 + index, attempt=2)
            self.assertNotEqual(first, trusted)
            self.assertNotEqual(rerun, trusted)
            self.assertNotEqual(first, rerun)
            groups.extend((first, rerun))
        self.assertEqual(len(groups), len(set(groups)))
        self.assertNotEqual(
            self.group(event="pull_request", repository="other/project"),
            self.group(event="pull_request"),
        )

    def test_policy_rejects_missing_cancelling_shared_source_scoped_or_unavailable_context_groups(self):
        for mutation in (
            "missing", "cancel", "missing-cancel", "shared", "source-scoped",
            "missing-attempt", "missing-run-id", "env-context", "steps-context",
            "changed-trust", "group-trust", "workflow-lock", "controls-lock",
        ):
            document = copy.deepcopy(self.workflow)
            job = document["jobs"]["fuzz"]
            concurrency = job["concurrency"]
            group = concurrency["group"]
            if mutation == "missing":
                job.pop("concurrency")
            elif mutation == "cancel":
                concurrency["cancel-in-progress"] = "true"
            elif mutation == "missing-cancel":
                concurrency.pop("cancel-in-progress")
            elif mutation == "shared":
                concurrency["group"] = "fuzz-generated-v2-trusted-owner/project-main"
            elif mutation == "source-scoped":
                concurrency["group"] = group.replace(
                    "format('fuzz-generated-v2-trusted-{0}-main', github.repository)",
                    "format('fuzz-generated-v2-trusted-{0}-{1}', github.repository, github.sha)",
                )
            elif mutation == "missing-attempt":
                concurrency["group"] = group.replace("-{2}',", "',").replace(", github.run_attempt", "")
            elif mutation == "missing-run-id":
                concurrency["group"] = group.replace("github.run_id", "github.run_attempt")
            elif mutation == "env-context":
                concurrency["group"] = "${{ env.TRUSTED_CORPUS && 'trusted' || github.run_id }}"
            elif mutation == "steps-context":
                concurrency["group"] = group.replace("github.repository", "steps.identity.outputs.sha256")
            elif mutation == "changed-trust":
                job["env"]["TRUSTED_CORPUS"] = "${{ github.ref == 'refs/heads/main' }}"
            elif mutation == "group-trust":
                concurrency["group"] = group.replace("github.event_name == 'schedule'", "github.event_name == 'pull_request'")
            elif mutation == "workflow-lock":
                document["concurrency"] = job.pop("concurrency")
            else:
                document["jobs"]["controls"]["concurrency"] = copy.deepcopy(concurrency)
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "fuzz"):
                check_workflow(document, "fuzz.yml")


if __name__ == "__main__":
    unittest.main()
