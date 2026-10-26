# Coordinated GitHub proof work

The coordinator owns publication cleanup, task boundaries, model confirmation,
integration and acceptance. GitHub workers implement bounded proof objectives.
The reusable [worker profile](../.github/agents/verus-proof-worker.agent.md)
requests **GPT-6.1 Sol** (`gpt-6.1-sol`) explicitly and forbids model fallback.

GitHub documents the `model` property in its
[custom-agent configuration reference](https://docs.github.com/en/copilot/reference/custom-agents-configuration),
but a configured custom agent is not confirmation of the top-level task model.
The [REST task API](https://docs.github.com/en/rest/agent-tasks/agent-tasks)
provides an explicit `model` parameter; omitting it uses automatic selection.
GitHub also documents [UI model-picker entrypoints](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/changing-the-ai-model).
Do not treat a profile, task prompt, session title or selected custom-agent name
as proof that the task uses GPT-6.1 Sol.

The profile must be available to GitHub before dispatch; merge a new profile
into the default branch for discovery. GitHub also documents branch-specific
profile versioning. Independently confirm the provider-reported selected model
before allowing proof implementation. Missing, unknown or different model
metadata leaves work blocked, not authorized to fall back.

## Issue-free dispatch

Prefer the documented REST task API with an explicit model and custom agent:

```sh
gh api --method POST agents/repos/microsoft/jbig2_rs/tasks \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  -f prompt='BOUNDED TASK WITH SCOPE, HOLDS AND ACCEPTANCE COMMANDS' \
  -f model=gpt-6.1-sol \
  -f custom_agent=verus-proof-worker \
  -f base_ref=main \
  -F create_pull_request=true
```

Use a complete bounded assignment, not the placeholder above. The endpoint
requires an eligible user-to-server token, Agent tasks permissions, and a model
allowed by the user's plan and organization. Authentication or model rejection
is a blocker, not permission to switch credentials, change policy or fall back.

For a same-task follow-up, pass the existing `head_ref` and
`create_pull_request=false`. The API looks up its open PR context and commits
to that branch instead of creating a duplicate proof PR.

The coordinator must inspect the returned task's actual session metadata:

```sh
gh api agents/repos/microsoft/jbig2_rs/tasks/TASK_ID \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  --jq '{id,state,custom_agent,sessions:[.sessions[]|{id,state,model}]}'
```

Confirm the active session reports GPT-6.1 Sol and the requested custom agent
before allowing implementation or granting progress credit. Retain the real
task/session IDs, exact provider model string and endpoint as a receipt. Do not
repeat creation after an ambiguous response; inspect the known task first.

This verification is coordinator-owned. The worker's bounded tool allowlist
does not expose GitHub PR/provider APIs: do not require it to fetch an
unavailable receipt or add credentials/permissions just for that check. The
coordinator can publish its independently observed receipt in the PR description
and qualify worker model reporting as coordinator-verified.

The CLI's `agent-task create --custom-agent` option selects a profile, not an
explicit top-level model. CLI 2.103.2 has no model-selection flag, so that
creation route is **not approved for this model-bound proof work**. Use the
explicit REST API above instead; a successful creation response alone does not
satisfy model confirmation.

Alternatively, use GitHub's agents tab or panel with this repository, worker
profile and **GPT-6.1 Sol** explicitly selected in the model picker. Complete
required organization SSO interactively; do not bypass authentication or request
passwords or authentication codes in chat.

Do not create tracking issues as a delegation mechanism. Stop before source/
proof edits if the coordinator cannot confirm the requested model; do not launch
another worker with the same unresolved preflight. Do not report a prepared
profile, completed model preflight or successful cloud workflow as proof
implementation.

The CLI can also inspect a known session:

```text
gh agent-task view SESSION_ID --repo microsoft/jbig2_rs \
  --json id,state,pullRequestUrl
```

Its listed JSON fields do not include the model. Keep provider model evidence
separate from these status fields and never infer it from the session's name.

## Task contracts

Each assignment specifies one proof objective, permitted files, existing
models/tests, acceptance commands and explicit stop conditions. Two suitable
independent objectives are:

- **Page/stripe state transitions:** exact acceptance, returned metadata and
  failure frames for the existing page-state operations. Use raw numeric
  metadata tests. This is not complete page rendering or lifecycle conformance.
- **Raw Huffman canonical assignment:** exact prefix construction/order over
  approved raw table-line inputs. Do not generate encoded custom-Huffman
  fixtures or infer full encoded-table/entropy conformance.

Limit concurrent workers to independent scopes. Shared checker registration,
traceability and evidence changes require coordinated rebasing/integration,
not competing edits accepted on stale bases. The coordinator does not repair
proof bodies by weakening contracts when a worker is blocked.

## Acceptance

Read the actual diff and results. Require fresh whole verification and a
compiled library, real compiling strong/weaker contrasts, native regression
sensitivity, unchanged trust/pin/resource policies and complete successful
current-head hosted evidence. Independently consume canonical artifacts and
raw coverage metrics. Keep PRs draft until these checks pass.

The capacity decision, paused fixture families, independent normative review,
seven unresolved official positives and release permissions are not delegated
away. Workers cannot self-approve, merge, publish, change visibility or contact
external maintainers. See [publication readiness](publication-readiness.md).
