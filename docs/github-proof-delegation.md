# Coordinated GitHub proof work

The coordinator owns publication cleanup, task boundaries, model confirmation,
integration and acceptance. GitHub workers implement bounded proof objectives.
The reusable [worker profile](../.github/agents/verus-proof-worker.agent.md)
requests **GPT-6.1 Sol** (`gpt-6.1-sol`) explicitly and forbids model fallback.

GitHub documents the `model` property in its
[custom-agent configuration reference](https://docs.github.com/en/copilot/reference/custom-agents-configuration),
but a configured custom agent is not confirmation of the top-level task model.
GitHub's [cloud-agent model-selection guide](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/changing-the-ai-model)
states that entrypoints without a model picker use **Auto**. Do not treat a
profile, task prompt, session title or selected custom-agent name as proof that
the task uses GPT-6.1 Sol.

The profile must be available to GitHub before dispatch; merge a new profile
into the default branch for discovery. GitHub also documents branch-specific
profile versioning. Independently confirm the provider-reported selected model
before allowing proof implementation. Missing, unknown or different model
metadata leaves work blocked, not authorized to fall back.

## Issue-free dispatch

Use GitHub's agents tab or agents panel, select this repository and the worker
profile, and explicitly select **GPT-6.1 Sol** in the model picker before
submitting the bounded task. These are documented issue-free entrypoints.
Complete any required organization SSO interactively; do not bypass
authentication or request passwords or authentication codes in chat.

The CLI's `agent-task create --custom-agent` option selects a profile, not an
explicit top-level model. CLI 2.103.2 has no model-selection flag, so that
creation route is **not approved for this model-bound proof work**. A successful
creation response alone does not satisfy model confirmation. Reconsider only
when a documented explicit-model option is available.

Do not create tracking issues as a delegation mechanism. Inspect the actual
task/session and provider-reported model metadata, then retain its real ID
for follow-up. Stop before source/proof edits if the requested model cannot be
confirmed; do not launch another worker with the same unresolved preflight.
Do not report a prepared profile, completed preflight or successful cloud
workflow as proof implementation.

The CLI can still inspect a known session after supported UI dispatch:

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
