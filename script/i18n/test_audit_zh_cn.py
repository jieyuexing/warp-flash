#!/usr/bin/env python3

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import audit_zh_cn


class AuditZhCnTests(unittest.TestCase):
    def decode(self, literal: str) -> str:
        match = audit_zh_cn.STATIC_CALL.search(f"Text::new({literal}, family, 12.)")
        self.assertIsNotNone(match)
        return audit_zh_cn.decode_rust_string(match)

    def test_decodes_rust_strings_and_line_continuations(self) -> None:
        self.assertEqual(self.decode(r'"Line\nTwo"'), "Line\nTwo")
        self.assertEqual(self.decode('"Line\\\n    Two"'), "LineTwo")
        self.assertEqual(self.decode(r'r#"literal \\n text"#'), r"literal \\n text")

    def test_masks_line_block_and_nested_comments(self) -> None:
        source = '''
// Text::new("line comment", family, 12.)
/* Text::new("block comment", family, 12.)
   /* Text::new("nested comment", family, 12.) */ */
Text::new("visible", family, 12.)
'''
        masked = audit_zh_cn.mask_rust_comments(source)
        matches = list(audit_zh_cn.STATIC_CALL.finditer(masked))
        self.assertEqual([audit_zh_cn.decode_rust_string(match) for match in matches], ["visible"])
        self.assertEqual(masked.count("\n"), source.count("\n"))

    def test_extracts_static_and_formatted_tui_spans_separately(self) -> None:
        source = '''
TuiText::from_spans([
    ("Static".to_owned(), primary),
    (format!("Count: {count}"), muted),
])
'''
        static, formatted = audit_zh_cn.extract_tui_span_occurrences(
            source, audit_zh_cn.mask_rust_comments(source), "crates/example.rs", {}
        )
        self.assertEqual([occurrence.source for occurrence in static], ["Static"])
        self.assertEqual([occurrence.source for occurrence in formatted], ["Count: {count}"])

    def test_resolves_ui_constants_and_text_and_icon_labels(self) -> None:
        source = '''
const BUTTON_LABEL: &str = "Invite a friend";
Text::new(BUTTON_LABEL, family, 12.);
TextAndIcon::new(Alignment::IconFirst, "New session", icon, size, spacing, icon_size);
'''
        searchable = audit_zh_cn.mask_rust_comments(source)
        constants = {
            match.group("name"): audit_zh_cn.decode_rust_string(match)
            for match in audit_zh_cn.CONST_DEFINITION.finditer(searchable)
        }
        const_call = audit_zh_cn.CONST_CALL.search(searchable)
        text_and_icon = audit_zh_cn.TEXT_AND_ICON_STATIC.search(searchable)

        self.assertIsNotNone(const_call)
        self.assertIsNotNone(text_and_icon)
        self.assertEqual(constants[const_call.group("name")], "Invite a friend")
        self.assertEqual(audit_zh_cn.decode_rust_string(text_and_icon), "New session")

    def test_catalog_decodes_escaped_fields(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            catalog_path = Path(directory) / "catalog.tsv"
            catalog_path.write_text(
                "Line\\nTwo\t行一\\n行二\nPath \\\\ value\t路径 \\\\ 值\n",
                encoding="utf-8",
            )
            catalog = audit_zh_cn.load_catalog(catalog_path)
        self.assertEqual(catalog["Line\nTwo"], "行一\n行二")
        self.assertEqual(catalog[r"Path \ value"], r"路径 \ 值")


if __name__ == "__main__":
    unittest.main()
