#!/usr/bin/env python3
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
COVERAGE = ROOT / "production-proof-coverage.json"
OUTPUT = ROOT / "proof-dispositions.json"

AUDITED_FORWARDERS = {
    "arithmetic::ArithmeticError::from",
    "decode::PageCollector::write_page",
    "document::Decoder::default",
    "generic_region::GenericRegionError::from",
    "generic_region::GenericRegionError::from#2",
    "generic_region::GenericRegionError::from#3",
    "halftone::HalftoneError::from",
    "halftone::HalftoneError::from#2",
    "halftone::HalftoneError::from#3",
    "huffman::HuffmanError::from",
    "huffman::HuffmanError::from#2",
    "huffman::HuffmanError::from#3",
    "mmr::MmrError::from",
    "limits::Limits::default",
    "symbol_text::SymbolTextError::from",
    "symbol_text::SymbolTextError::from#2",
    "symbol_text::SymbolTextError::from#3",
    "symbol_text::SymbolTextError::from#4",
    "symbol_text::SymbolTextError::from#5",
    "symbol_text::SymbolTextError::from#6",
    "symbol_text::SymbolTextError::from#7",
}

REASONS = {
    "src/arithmetic.rs": "Pinned Verus does not directly verify the remaining standard From and formatting trait implementations; Annex A decoder algorithms and default context construction are directly verified.",
    "src/bitmap.rs": "Pinned vstd has no specification for fallible Vec reservation used by bitmap allocation.",
    "src/decode.rs": "Pinned Verus lacks fallible allocation, trait callback, page-sink, and composed resource-lifetime specifications required by remaining orchestration paths.",
    "src/document.rs": "Pinned Verus lacks Read, iterator-closure, and fallible collection specifications used by remaining document parsing paths.",
    "src/error.rs": "Pinned Verus does not support formatting, dynamic Error machinery, or std::io::ErrorKind conversion.",
    "src/generic_region.rs": "Pinned Verus lacks fallible Vec/slice and stateful callback specifications required by complete raster decoder paths.",
    "src/halftone.rs": "Pinned Verus lacks allocation, bitmap mutation, and generic decoder callback specifications for end-to-end halftone paths.",
    "src/huffman.rs": "Pinned Verus lacks fallible Vec allocation, size_of reasoning, and array-to-slice coercion specifications.",
    "src/mmr.rs": "Pinned Verus lacks fallible transition allocation and complete Bitmap allocation contracts.",
    "src/segment_data.rs": "Pinned Verus lacks fallible Vec allocation required by profile-list materialization.",
    "src/segment_header.rs": "Pinned Verus lacks fallible Vec allocation required by retained-reference materialization.",
    "src/symbol_text.rs": "Pinned Verus lacks fallible Vec, RefCell, dynamic callback, and upstream bitmap allocation specifications for complete dictionary/text orchestration."
}


coverage = json.loads(COVERAGE.read_text(encoding="utf-8"))
functions = []
for entry in coverage["functions"]:
    if entry["status"] != "incomplete":
        continue
    if entry["id"] in AUDITED_FORWARDERS:
        status = "audited-forwarder"
        reason = (
            "One-line standard trait wrapper around an already verified "
            "typed constructor or conversion; body hash changes invalidate this audit."
        )
    else:
        status = "reviewed-verifier-limitation"
        reason = REASONS.get(entry["file"])
        if reason is None:
            raise RuntimeError(f"no reviewed limitation reason for {entry['file']}")
    functions.append(
        {
            "id": entry["id"],
            "file": entry["file"],
            "file_sha256": entry["file_sha256"],
            "status": status,
            "reason": reason,
        }
    )

document = {
    "schema_version": 1,
    "policy": "Every unverified production function must have a reviewed, body-hash-bound disposition. A source change makes the disposition stale.",
    "functions": functions,
}
OUTPUT.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
print(f"wrote {len(functions)} proof dispositions")
