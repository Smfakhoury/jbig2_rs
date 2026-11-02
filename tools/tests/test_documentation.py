import re
import unittest
from pathlib import Path
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[2]


def documentation_files():
    return [
        ROOT / "README.md", ROOT / "RELEASING.md", ROOT / "SECURITY.md",
        *sorted((ROOT / "docs").glob("*.md")),
        ROOT / "specs/README.md", ROOT / "validation/README.md",
        ROOT / "validation/comparison/README.md", ROOT / "verification/stdlib/STATUS.txt",
    ]


class DocumentationTests(unittest.TestCase):
    def test_reader_guides_do_not_include_implementation_conversation(self):
        discussion = re.compile(
            r"\b(?:CELA|JIT|coordinator|owner[- ]reported|owner[- ]approved|"
            r"owner authoriz\w*|the owner(?:'s)?|our discussion|"
            r"permission renewal|session-state|owner adjudication)\b",
            re.IGNORECASE,
        )
        stale_status = re.compile(
            r"(?:this|the) crate is unpublished|repository remains private|"
            r"runtime/proof divergence remain(?:s)? open",
            re.IGNORECASE,
        )
        for path in documentation_files():
            with self.subTest(path=path.relative_to(ROOT)):
                text = path.read_text()
                self.assertIsNone(discussion.search(text))
                self.assertIsNone(stale_status.search(text))

    def test_relative_documentation_links_resolve(self):
        for path in documentation_files():
            for target in re.findall(r"\[[^\]\n]*\]\(([^)\n]+)\)", path.read_text()):
                target = target.strip().split()[0].strip("<>")
                parsed = urlsplit(target)
                if parsed.scheme or parsed.netloc:
                    continue
                linked = (path.parent / unquote(parsed.path)).resolve() if parsed.path else path
                with self.subTest(path=path.relative_to(ROOT), target=target):
                    self.assertTrue(linked.is_relative_to(ROOT))
                    self.assertTrue(linked.exists(), target)
                    if linked.suffix == ".md" and parsed.fragment:
                        headings = re.findall(r"^#{1,6}\s+(.+)$", linked.read_text(), re.MULTILINE)
                        anchors = {
                            re.sub(r"[^\w -]", "", heading.lower()).replace(" ", "-")
                            for heading in headings
                        }
                        self.assertIn(unquote(parsed.fragment), anchors)

    def test_public_api_docs_have_no_stale_publication_status(self):
        crate_docs = (ROOT / "src/lib.rs").read_text()
        self.assertNotIn("this crate is unpublished", crate_docs)
        self.assertIn("not yet suitable", crate_docs)
        self.assertIn("for untrusted production documents", crate_docs)
        limits_docs = (ROOT / "src/limits.rs").read_text()
        self.assertNotIn("approved Phase 1", limits_docs)


if __name__ == "__main__":
    unittest.main()
