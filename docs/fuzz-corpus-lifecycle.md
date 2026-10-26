# Generated fuzz corpus lifecycle

This operational lane applies only to the nine targets already declared in
`fuzz/Cargo.toml`. It does not add packet grammars, decoder input generators,
targets, coverage floors or conformance claims. Inputs originate in the existing
`fuzz_seeds` example and subsequent bounded first-party campaigns. Do not supply
third-party vectors, copyrighted assets or private corpora.

## Reproduction

Use Linux, Python 3.11+, `nightly-2026-10-12` and `cargo-fuzz 0.13.2`, as pinned
in [`config/toolchain.json`](../config/toolchain.json). Every output directory
must be fresh. Standard temporary files used by libFuzzer or isolated controls
must reside in an authorized local
workspace location; set `TMPDIR` accordingly.

Validate the minimal existing generated corpus, including real minimization and
exact replay of every input before and after minimization:

```sh
python -B tools/maintain_fuzz_corpus.py --generated \
  --output-dir target/generated-corpus-validation
```

Validate a completed nine-target campaign:

```sh
python -B tools/run_fuzz.py run --profile smoke \
  --corpus-dir target/fuzz-corpus --output-dir target/fuzz-run
python -B tools/maintain_fuzz_corpus.py \
  --corpus-dir target/fuzz-corpus \
  --campaign-report target/fuzz-run/report.json \
  --output-dir target/fuzz-maintenance
```

Without `--install`, the input corpus is never replaced. The maintenance output
contains an unchanged `preserved-input/` copy and a separate `minimized/` copy.
After **all nine targets** pass, `--install` moves the original input directory
to `replaced-input/` and renames the minimized copy into its former location.
If the second rename fails, the original directory is restored. This requires
both directories on the same filesystem; no cross-filesystem copying fallback
or partial per-target promotion is permitted.

Single-input replay remains available through the ordinary runner:

```sh
python -B tools/run_fuzz.py replay --targets file_header_arbitrary \
  --artifact target/fuzz-corpus/file_header_arbitrary/generated-v1-page \
  --output-dir target/fuzz-replay
```

## Actual pinned minimizer semantics and evidence

`cargo +nightly-2026-10-12 fuzz cmin TARGET CORPUS -- FLAGS` builds the target,
merges into a temporary directory owned by cargo-fuzz inside the fuzz project,
and **replaces CORPUS** on success. It must never receive the original corpus.
Both the actual `cmin --help` output and all operation logs are retained.

Real pinned runs produce `MERGE-OUTER`/`MERGE-INNER` records and a `#N DONE`
record, **not** `stat::number_of_executed_units` for the outer minimizer.
Validation requires one unambiguous complete merge, one attempt, exactly the
inventoried input count, positive feature/edge counts matching its completion
record, and an output file count matching its result record. Output contents
must be a nonempty multiset subset of input hashes, with no byte growth.
Missing output, missing/duplicated statistics, retry/recovery, nonzero exits and
timeouts are failures even if another part of the log appears successful.

Before and after minimization, `cargo fuzz run TARGET FILE... -- -runs=1`
executes each explicit file once, without mutation. Validation checks the exact
ordered `Running:` and `Executed ... in ... ms` records and the unique positive
final execution counter against the full inventory. Any reproducer fails the
operation. This differs deliberately from replaying a directory, whose
initialization may discard or deduplicate inputs and execute an extra empty
input.

`report.json` records:

- Exact before, staged-before and after names, sizes and SHA-256 hashes, plus
  collection summaries.
- Git revision, source digest, dirty state, pinned toolchain, verbose rustc
  identity, cargo and cargo-fuzz versions.
- Every command, exit status, timeout flag, duration and retained diagnostic log.
- Actual merge processed-unit/retained-file/feature/edge counts and exact replay
  counts; values are measured, not assumed from a zero exit.
- Campaign-report and raw seed/build/per-target log hashes. Campaign status,
  source revision/digest, tool versions, full nine-target membership, counters,
  raw final counters and current corpus summaries must agree.

The feature/edge values describe **libFuzzer's instrumented feedback only**.
They do not establish preservation of source-line coverage. Minimized file
selection can vary; the report records each actual result instead of asserting
a predetermined reduction ratio.

## Fail-closed bounds

Collections must contain exactly the nine supported target directories, each
nonempty. Symlinks (including path ancestors), nonregular files, subdirectories,
empty input files and unsupported names are rejected. Names are bounded ASCII
letters/digits/`._-`, at most 128 characters. Inventories and owned copies use
bounded reads; copies cannot follow input-file symlinks and must match recorded
sizes/hashes. Changed input, preserved copy, source, campaign report or raw
campaign evidence prevents promotion.

| Bound | Limit |
| --- | --- |
| Individual input | 16,384 bytes, 5 seconds |
| libFuzzer RSS | 1,024 MiB |
| Files per target | 4,096 |
| Total collection | 16,384 files, 128 MiB |
| Build operation | 120 seconds |
| Each merge | 30 seconds, at most 4,097 requested runs |
| Each exact replay invocation | 20 seconds, one execution per explicit file |
| Entire maintenance process | 175 seconds, including setup/build/copies/checks |
| Individual diagnostic accepted as evidence | 16 MiB |

The stricter maintenance collection bounds are intentional. An unsupported or
over-budget campaign corpus is retained for review and fails closed rather than
being silently truncated or partially installed. Each subprocess has a
process-group deadline and termination escalation. Remaining total time caps
each operation, reserving termination/reporting time; per-operation maxima are
not additive allowances beyond the total deadline.

## Workflow trust and budget

The failure/replay controls run in a separate read-only job, followed by real
existing-generated maintenance validation. Control-copy source identity must
equal production before injection and after restoration, and differ during
the generated benign-marker failure and its replay. Production target files
are not modified.

The campaign job retains its **60-minute cap**. Explicit step caps sum to 60:

| Campaign job phase | Maximum minutes |
| --- | ---: |
| Checkout / Python / Rust / pinned cargo-fuzz plus source identity | 1 + 1 + 1 + 2 |
| Cache restore | 1 |
| Nine-target campaign | 49 |
| Maintenance and promotion | 3 |
| Cache save / artifact upload | 1 + 1 |
| **Total** | **60** |

The campaign's own deadline is 2,935 seconds: nine 300-second extended runs use
2,700 seconds, leaving 235 seconds for seed generation, builds, startup,
overshoot and validation. Maintenance has its independent 175-second deadline.
Each Python process reserves five seconds inside its enclosing step cap.
Exhausted deadlines fail; they cannot expand the job or shorten a successful
extended campaign silently. Tests check the arithmetic against the actual YAML
and runner constants. This is an execution budget, not a guarantee that slow
machines or a maximal corpus finish.

Only scheduled or explicitly dispatched runs on `refs/heads/main`, with `main`
as the repository default branch, may restore a retained corpus, minimize and
promote it, or save the cache. PRs and other branches generate their own seeds
and cannot access or write the trusted cache through these steps. Permissions
remain `contents: read`; no privileged PR trigger is used. Cache v2 keys bind the
entire existing source identity, including decoder/harness/fixture/manifests,
toolchain configuration, runner, maintenance, control-copy script and workflow.
There is no fallback to a different source identity or the former cache scheme.
Artifacts retain reports, corpora and diagnostics on success or failure.

### Serialize trusted cache producers

The `fuzz` job uses a repository/main-specific concurrency group for exactly the
same trusted predicate as the cache steps. The group is independent of source
SHA, run ID, attempt and profile: scheduled and manual main producers cannot
restore the same prior cache concurrently and later overwrite one another's
feedback. The lock covers checkout through cache save and artifact upload, not
only minimization. `cancel-in-progress: false` protects the running producer.
The controls job remains independent.

Pull requests run once rather than again on the same feature-branch push.
Push triggers are restricted to `main` and all tags, with manual branch
dispatch still available. Daily extended campaigns and the exact trusted
schedule/manual-main predicate are unchanged; no PR or tag gains cache
promotion rights through the trigger change.

Every untrusted run uses a distinct repository/run-ID/attempt fallback group.
PRs, pushes and nondefault-branch dispatches cannot join, queue behind, or cancel
the trusted group, including when rerun. The job expression uses only `github`
context, not unavailable job-concurrency `env` or `steps` context. The workflow
policy checker rejects changed trust/group/cancellation behavior or moving the
lock outside the producer job.

GitHub's default concurrency queue is bounded (one running and one pending job
per group): an additional producer may replace an already pending producer.
Canceled pending jobs have not restored or saved a corpus. Ordering follows
when jobs start waiting, not workflow dispatch order; independent controls can
finish in either order. See [GitHub's concurrency
documentation](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).
Hosted cache-lineage acceptance requires paired same-SHA trusted runs that
verify actual queued/running transitions and all nine restored/promoted
inventories. Local policy tests are not hosted lineage evidence; the existence
of this concurrency configuration does not claim those runs occurred.
The execution budgets are unchanged.
