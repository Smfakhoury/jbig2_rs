# Coordinated GitHub proof work

The coordinator owns publication cleanup, task boundaries, model confirmation,
integration and acceptance. GitHub workers implement bounded proof objectives.
The reusable [worker profile](../.github/agents/verus-proof-worker.agent.md)
requests **GPT-6.1 Sol** (`gpt-6.1-sol`) explicitly and forbids model fallback.

GitHub documents the `model` property in its
[custom-agent configuration reference](https://docs.github.com/en/copilot/reference/custom-agents-configuration).
The profile must be available on the task's base branch before dispatch. The
coordinator must confirm that GitHub accepts the profile/model; a task prompt
alone is not evidence of the serving model. If unavailable or not verifiable,
leave dispatch blocked rather than silently use another model or environment.

## Issue-free dispatch

The GitHub CLI must support `agent-task`. Older installations may not; inspect
the installed help first. Use a session-local, checksum-verified official CLI
when an update is necessary rather than replacing a shared installation.

```text
gh agent-task create --repo microsoft/jbig2_rs --base main \
  --custom-agent verus-proof-worker --from-file -
```

Provide the bounded task on standard input. This creates agent work directly;
do not create tracking issues as a delegation mechanism. Inspect the created
task/session and provider-reported model metadata, then retain its actual ID
for follow-up. Do not report a prepared profile as a running worker.

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
