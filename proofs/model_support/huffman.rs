// Included in huffman_spec; the closed model is not made open for callers.
verus! {

pub(super) proof fn full_prefix(lines: Seq<super::HuffmanLine>)
    ensures lines.take(lines.len() as int) == lines,
{
    vstd::assert_seqs_equal!(lines.take(lines.len() as int), lines);
}

pub(super) proof fn canonical_assignment_definition()
    ensures
        forall|lines: Seq<super::HuffmanLine>, table: &super::HuffmanTable|
            #[trigger] assignment_contents(lines, table) == (
                table.lines@ == lines
                && histogram(lines, table.length_count@, lines.len())
                && 1 <= table.max_prefix_len <= 255
                && table.length_count@[table.max_prefix_len as int] > 0
                && (forall|length: int| table.max_prefix_len < length < 256 ==>
                    table.length_count@[length] == 0)
                && table.ordered_lines@.len()
                    == ordered_before(lines, table.max_prefix_len as nat + 1).len()),
        forall|lines: Seq<super::HuffmanLine>, table: &super::HuffmanTable|
            #[trigger] assignment_codes(lines, table) == (
                table.first_code@[0].limbs@ == seq![0u64, 0u64, 0u64, 0u64]
                && forall|length: int| 1 <= length <= table.max_prefix_len ==>
                    #[trigger] first_prefix(lines, length as nat) == Some(table.first_code@[length])
                    && (table.length_count@[length] > 0 ==>
                        canonical_range_fits(table.first_code@[length], table.length_count@[length], length as usize))),
        forall|lines: Seq<super::HuffmanLine>, table: &super::HuffmanTable|
            #[trigger] assignment_starts(lines, table) == (
                forall|length: int| 1 <= length <= table.max_prefix_len
                    && table.length_count@[length] > 0 ==>
                        table.first_line@[length] == ordered_before(lines, length as nat).len()),
        forall|lines: Seq<super::HuffmanLine>, table: &super::HuffmanTable|
            #[trigger] assignment_order(lines, table) == (
                table.ordered_lines@ == ordered_before(lines, table.max_prefix_len as nat + 1)),
{
    reveal(assignment_contents);
    reveal(assignment_codes);
    reveal(assignment_starts);
    reveal(assignment_order);
}

pub(super) proof fn assignment_complete(lines: Seq<super::HuffmanLine>, table: &super::HuffmanTable)
    requires
        table.lines@ == lines,
        histogram(lines, table.length_count@, lines.len()),
        1 <= table.max_prefix_len <= 255,
        table.length_count@[table.max_prefix_len as int] > 0,
        forall|length: int| table.max_prefix_len < length < 256 ==>
            table.length_count@[length] == 0,
        assignment_prefix(
            lines, table.first_code@, table.first_line@, table.ordered_lines@,
            table.max_prefix_len as nat + 1,
        ),
    ensures canonical_assignment(lines, table),
{
    canonical_assignment_definition();
    assert forall|length: int| 1 <= length <= table.max_prefix_len implies
        #[trigger] first_prefix(lines, length as nat) == Some(table.first_code@[length])
        && (table.length_count@[length] > 0 ==>
            canonical_range_fits(table.first_code@[length], table.length_count@[length], length as usize)) by {
        assert(table.length_count@[length] == bucket(lines, length as nat, lines.len()).len());
    }
    assert forall|length: int| 1 <= length <= table.max_prefix_len
        && table.length_count@[length] > 0 implies
            table.first_line@[length] == ordered_before(lines, length as nat).len() by {
        assert(first_prefix(lines, length as nat) == Some(table.first_code@[length]));
        assert(table.length_count@[length] == bucket(lines, length as nat, lines.len()).len());
    }
}

pub(super) proof fn histogram_empty(lines: Seq<super::HuffmanLine>, counts: Seq<u32>)
    requires counts.len() == 256, forall|i: int| 0 <= i < 256 ==> counts[i] == 0,
    ensures histogram(lines, counts, 0),
{
    assert forall|length: int| 0 <= length < 256 implies
        counts[length] == bucket(lines, length as nat, 0).len() by {
    }
}

pub(super) proof fn histogram_step(
    lines: Seq<super::HuffmanLine>, before: Seq<u32>, after: Seq<u32>, scanned: nat,
)
    requires
        histogram(lines, before, scanned),
        scanned < lines.len(),
        after.len() == 256,
        forall|length: int| 0 <= length < 256 ==> after[length] as int ==
            before[length] as int
                + if length != 0 && lines[scanned as int].prefix_len == length { 1int } else { 0int },
    ensures histogram(lines, after, scanned + 1),
{
    assert forall|length: int| 0 <= length < 256 implies
        after[length] == bucket(lines, length as nat, scanned + 1).len() by {
        reveal(bucket);
    }
}

pub(super) proof fn empty_bucket(
    lines: Seq<super::HuffmanLine>, length: nat, scanned: nat, prefix: Seq<u32>,
)
    requires scanned <= lines.len(), bucket(lines, length, scanned).len() == 0,
    ensures bucket(lines, length, scanned) == Seq::<u32>::empty(),
        prefix + bucket(lines, length, scanned) == prefix,
{
    vstd::assert_seqs_equal!(bucket(lines, length, scanned), Seq::<u32>::empty());
    vstd::assert_seqs_equal!(prefix + bucket(lines, length, scanned), prefix);
}

pub(super) proof fn bucket_step(
    lines: Seq<super::HuffmanLine>, length: nat, scanned: nat,
    prefix: Seq<u32>, before: Seq<u32>, after: Seq<u32>,
)
    requires
        scanned < lines.len(),
        before == prefix + bucket(lines, length, scanned),
        after == if length != 0 && lines[scanned as int].prefix_len == length {
            before.push(scanned as u32)
        } else { before },
    ensures after == prefix + bucket(lines, length, scanned + 1),
{
    reveal(bucket);
    vstd::assert_seqs_equal!(after, prefix + bucket(lines, length, scanned + 1));
}

pub(super) proof fn assignment_step(
    lines: Seq<super::HuffmanLine>, counts: Seq<u32>,
    old_codes: Seq<super::WideCode>, old_starts: Seq<u32>, old_order: Seq<u32>,
    codes: Seq<super::WideCode>, starts: Seq<u32>, order: Seq<u32>, length: nat,
)
    requires
        histogram(lines, counts, lines.len()),
        assignment_prefix(lines, old_codes, old_starts, old_order, length),
        length < 256,
        codes.len() == 256,
        codes == old_codes.update(length as int, codes[length as int]),
        canonical_first_code(old_codes[length - 1], counts[length - 1]) == Some(codes[length as int]),
        starts.len() == 256,
        forall|i: int| 0 <= i < 256 && i != length ==> starts[i] == old_starts[i],
        counts[length as int] > 0 ==> starts[length as int] == old_order.len()
            && canonical_range_fits(codes[length as int], counts[length as int], length as usize),
        order == old_order + bucket(lines, length, lines.len()),
    ensures assignment_prefix(lines, codes, starts, order, length + 1),
{
    if length == 1 {
        assert(ext_equal(old_codes[0].limbs, [0u64, 0u64, 0u64, 0u64]));
        assert(first_prefix(lines, 0) == Some(old_codes[0]));
    } else {
        assert(1 <= length - 1 < length);
        assert(first_prefix(lines, (length - 1) as nat) == Some(old_codes[length - 1]));
    }
    assert(counts[length - 1] == bucket(lines, (length - 1) as nat, lines.len()).len());
    assert(first_prefix(lines, length) == Some(codes[length as int]));
    assert(order == ordered_before(lines, length + 1));
    assert forall|i: int| 1 <= i < length + 1 implies
        first_prefix(lines, i as nat) == Some(codes[i])
        && (bucket(lines, i as nat, lines.len()).len() > 0 ==>
            starts[i] == ordered_before(lines, i as nat).len()
            && canonical_range_fits(codes[i], bucket(lines, i as nat, lines.len()).len() as u32, i as usize)) by {
        if i != length {
            assert(codes[i] == old_codes[i]);
        }
    }
}

pub(super) proof fn table_has_oob_definition(table: &super::HuffmanTable)
    ensures table_has_oob(table) == exists|index: int|
        0 <= index < table.lines@.len()
        && table.lines@[index].kind == super::LineKind::OutOfBand,
{
    reveal(table_has_oob);
}

}
