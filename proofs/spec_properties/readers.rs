// Source-only author navigation; named purposes are not normative approval.
// Guide, official acquisition and source identity: ../../specs/README.md
// All requirement links and inherited locators: ../../validation/traceability/requirements.json
// REQUIREMENT CONTEXT is not a direct normative equation, including for proof/policy helpers.

use vstd::prelude::*;

verus! {

// Model navigation: spec::bits_limit [proof-support]
// Purpose: Doubling bound used to justify reader accumulation.
// Audit: reader-bits-limit
// REQUIREMENT CONTEXT: T88-BITS-MSB-FIRST | ITU-T T.88 (08/2018) | 5.4.1
// Fidelity: unresolved; no independent normative approval.
pub(crate) open spec fn bits_limit(count: nat) -> nat
    decreases count,
{
    if count == 0 { 1 } else { 2 * bits_limit((count - 1) as nat) }
}

pub(crate) proof fn bits_limit_monotone(first: nat, last: nat)
    requires first <= last,
    ensures bits_limit(first) <= bits_limit(last),
    decreases last,
{
    if first < last {
        bits_limit_monotone(first, (last - 1) as nat);
    }
}

pub(crate) proof fn bits_value_bound(bytes: Seq<u8>, start: nat, count: nat)
    requires start + count <= bytes.len() * 8,
    ensures crate::spec::readers::bits_value(bytes, start, count) < bits_limit(count),
    decreases count,
{
    if count > 0 {
        bits_value_bound(bytes, start, (count - 1) as nat);
        let position = start + count - 1;
        let byte = bytes[(position / 8) as int];
        let shift = (7 - position % 8) as u8;
        assert(((byte >> shift) & 1) <= 1) by (bit_vector);
    }
}

pub(crate) proof fn bits_value_append_one(bytes: Seq<u8>, start: nat, count: nat)
    requires start + count < bytes.len() * 8,
    ensures
        crate::spec::readers::bits_value(bytes, start, count + 1)
            == crate::spec::readers::bits_value(bytes, start, count) * 2
                + crate::spec::readers::bits_value(bytes, start + count, 1),
{
    reveal_with_fuel(crate::spec::readers::bits_value, 2);
}

}
