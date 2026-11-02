# Reviewing models and proof claims

An implementation proof establishes its submitted contract under its
preconditions and trusted interfaces. A standard-to-model review asks whether
that contract expresses the intended JBIG2 behavior. These are separate
questions; implementation tests or internal model theorems do not answer both.

Start with the [model guide](../specs/README.md) and
[auditor index](verus-auditor-index.md). The
[ledger](../validation/traceability/requirements.json) records selected
requirements, source locators, evidence strengths, assumptions and gaps.

## Review a bounded property

Choose a concrete property such as arithmetic BYTEIN, an MMR row transition or
a text-placement result. Compare the pinned reference and model independently
of the executing implementation, then check the implementation's actual
preconditions/postconditions.

Challenge:

- Preconditions that exclude legitimate supported inputs.
- Vacuous or success-only claims that permit rejecting every valid input.
- Implementation-table membership presented as independent table fidelity.
- Bit/coordinate order, marker and initial/final boundary interpretation.
- Missing caller obligations or incomplete state/error frames.
- Resource/error policy presented as a standard requirement.
- Tests that derive both expected and actual output through the same code.

Record what was checked, the source/model identities, findings and unchecked
scope. A local property result is not an end-to-end conformance certificate.
External reference documents and assets have separate use/reproduction terms.

## Generate a source-bound review packet

```sh
python3 -B tools/check_traceability.py \
  --audit-requirement T88-ARITHMETIC-BYTEIN-BRANCH \
  --audit-requirement T88-ARITHMETIC-INITDEC-BYTEIN \
  --audit-packet target/traceability/bytein-packet.json \
  --output target/traceability/reference-report.json
```

The packet binds the selected rows, definitions, standard hashes and conservative
source/configuration dependencies. It contains no standard text and is labelled
`unreviewed`; generating it supplies no new theorem or review conclusion.
Proof-interface files and their Cargo/control inputs are included in the
snapshot; generated caches are not.

Check the packet against a separately retained expected hash:

```sh
python3 -B tools/check_traceability.py \
  --check-audit-packet target/traceability/bytein-packet.json \
  --expected-audit-sha256 EXPECTED_SHA256 \
  --output target/traceability/packet-check.json
```

Changed scope, inputs or asserted status invalidate the packet. Do not derive
the expected hash from a replacement packet itself. Reports stay under local
`target/` directories and cannot overwrite inputs. Hashes identify exact bytes,
including line endings.

### Optional GitHub source-review receipts

The checker can authenticate a human GitHub review of the exact source packet
when `policy.source_review_authority` is configured in
[`source-provenance.json`](../validation/source-provenance.json).
The review must target the exact current PR head and contain the packet's
separately retained pin:

```text
JBIG2-SOURCE-REVIEW-SHA256: EXPECTED_SHA256
```

```sh
python3 -B tools/check_traceability.py \
  --check-audit-packet target/traceability/bytein-packet.json \
  --expected-audit-sha256 EXPECTED_SHA256 \
  --source-review-pr PR_NUMBER --source-review-id REVIEW_ID \
  --output target/traceability/source-review.json
```

This requires a clean committed checkout and authenticated read access through
`gh`. The configured identities come from the default branch, not the submitted
packet. Wrong heads/pins, self-reviews, bot reviews, dismissed/superseded
decisions and incomplete history fail checks.

This receipt establishes a source-snapshot review only, not a standard-translation
certificate or permission to redistribute external materials.

## Source navigation

`model_classifications` lists exact declaration families and their purpose:
project result, policy, representation invariant, proof support or candidate
standard-facing model. These are navigation categories, not proof strengths.
The inventory covers registered definitions plus spec functions in `specs/`
and `proofs/model_support/`; other helpers are not silently included.

Each declaration's adjacent `Model navigation`, `Audit` and
`REQUIREMENT CONTEXT` comments connect source to the ledger. Unregistered
helpers expose missing requirement links. These comments are reading aids, not
independently checked citations.

The checker rejects missing/duplicate/ambiguous declarations, stale hashes,
wrong-purpose markers and misplaced comments. Registry and project-source
digests bind exact identities, module paths and all Rust source under `src/`,
`specs/` and `proofs/`. A hash update is not evidence that a changed model is
semantically equivalent.

### Documentation-only source changes

Before rebinding a comment-only change, compare the complete source sets
against the exact base. Parse every Rust file and compare non-comment token
leaves, including literal contents and all cfg branches. Preserve doctest code
and every function/loop contract. A string/comment masker alone cannot
establish this equivalence.

Then update only affected byte identities and explicitly record the scope of
the rebinding. It does not add a proof, complete a requirement or renew an
unrelated review. Runtime/proof checks still run on the resulting source.

## Validate model changes

Changes to semantics, tables, guards or preconditions need corresponding
source/model review, tests and new proof evidence. Keep unresolved gaps visible,
then verify the implementation against the resulting model. Do not silently
weaken the target to make a proof succeed.

Run whole verification/compilation, contract sensitivity, native regressions,
coverage and distribution checks on the final source. See
[validation](../validation/README.md) and
[verification limitations](verifier-limitations.md).
