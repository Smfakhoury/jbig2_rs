"""Guard the declaration attachment of the partitioned generic word oracle."""

from pathlib import Path
import re
import sys
import unittest


TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))

from rust_source import parse


WORD_ATTRIBUTES = (
    "#[cfg(test)]",
    "#[allow(clippy::ptr_arg)]",
    """#[cfg_attr(verus_keep_ghost, verus_spec(out =>
    requires
        rows <= old(raster).height,
        crate::arithmetic::arithmetic_spec::decoder_valid(old(decoder)),
        generic_region_spec::contexts_valid(old(contexts)@),
    ensures
        out.is_ok() ==> final(raster).width == old(raster).width,
        out.is_ok() ==> final(raster).height == old(raster).height,
        out.is_ok() ==> final(raster).stride_words == old(raster).stride_words,
        generic_region_spec::contexts_valid(final(contexts)@),
        out.is_ok() ==> crate::arithmetic::arithmetic_spec::decoder_valid(final(decoder)),
))]""",
    "#[allow(clippy::too_many_lines)]",
)
STORAGE_ATTRIBUTES = (
    "#[cfg(test)]",
    "#[cfg_attr(verus_keep_ghost, verus_verify)]",
)


class GenericOracleContractTests(unittest.TestCase):
    def setUp(self):
        directory = TOOLS.parent / "src/generic_region/tests"
        self.word_source = (directory / "word_raster_oracle.rs").read_text()
        self.ring_source = (directory / "row_ring_oracle.rs").read_text()

    def assert_attached(self, source, name, expected):
        attributes = []
        for node in parse(source.encode()).named_children:
            if node.type == "attribute_item":
                attributes.append(node.text.decode())
                continue
            if node.type in {"line_comment", "block_comment"}:
                continue
            if (
                node.type == "function_item"
                and node.child_by_field_name("name").text.decode() == name
            ):
                self.assertEqual(
                    [re.sub(r"\s+", "", attribute) for attribute in attributes],
                    [re.sub(r"\s+", "", attribute) for attribute in expected],
                    name,
                )
                return
            attributes = []
        self.fail(f"missing oracle helper: {name}")

    def test_complete_contract_attributes_stay_on_their_original_helpers(self):
        self.assert_attached(
            self.word_source, "decode_nominal_template0_words", WORD_ATTRIBUTES,
        )
        self.assert_attached(
            self.ring_source, "row_ring_storage_bytes", STORAGE_ATTRIBUTES,
        )

    def test_original_wrong_contract_attachment_is_rejected_on_both_helpers(self):
        displaced = "\n".join(WORD_ATTRIBUTES[:3]) + "\n"
        storage = "\n".join(STORAGE_ATTRIBUTES) + "\npub(super) fn row_ring_storage_bytes"
        self.assertEqual(self.word_source.count(displaced), 1)
        self.assertEqual(self.ring_source.count(storage), 1)
        wrong_word = self.word_source.replace(displaced, "", 1)
        wrong_ring = self.ring_source.replace(storage, displaced + storage, 1)
        for source, name, expected in (
            (wrong_word, "decode_nominal_template0_words", WORD_ATTRIBUTES),
            (wrong_ring, "row_ring_storage_bytes", STORAGE_ATTRIBUTES),
        ):
            with self.subTest(helper=name), self.assertRaises(AssertionError):
                self.assert_attached(source, name, expected)


if __name__ == "__main__":
    unittest.main()
