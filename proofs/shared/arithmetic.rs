use vstd::prelude::*;

verus! {

pub(super) proof fn qe_transition_valid(index: int)
    requires
        0 <= index < 47,
    ensures
        super::QE_STATES@[index].next_mps < 47,
        super::QE_STATES@[index].next_lps < 47,
        super::QE_STATES@[index].qe > 0,
        super::QE_STATES@[index].qe <= 0x5601,
{
    assert(super::QE_STATES@.len() == 47);
    if index == 0 {} else if index == 1 {} else if index == 2 {}
    else if index == 3 {} else if index == 4 {} else if index == 5 {}
    else if index == 6 {} else if index == 7 {} else if index == 8 {}
    else if index == 9 {} else if index == 10 {} else if index == 11 {}
    else if index == 12 {} else if index == 13 {} else if index == 14 {}
    else if index == 15 {} else if index == 16 {} else if index == 17 {}
    else if index == 18 {} else if index == 19 {} else if index == 20 {}
    else if index == 21 {} else if index == 22 {} else if index == 23 {}
    else if index == 24 {} else if index == 25 {} else if index == 26 {}
    else if index == 27 {} else if index == 28 {} else if index == 29 {}
    else if index == 30 {} else if index == 31 {} else if index == 32 {}
    else if index == 33 {} else if index == 34 {} else if index == 35 {}
    else if index == 36 {} else if index == 37 {} else if index == 38 {}
    else if index == 39 {} else if index == 40 {} else if index == 41 {}
    else if index == 42 {} else if index == 43 {} else if index == 44 {}
    else if index == 45 {} else {}
}

}
