// Included in huffman_spec; the closed model is not made open for callers.
verus! {

pub(super) proof fn table_has_oob_definition(table: &super::HuffmanTable)
    ensures table_has_oob(table) == exists|index: int|
        0 <= index < table.lines@.len()
        && table.lines@[index].kind == super::LineKind::OutOfBand,
{
    reveal(table_has_oob);
}

}
