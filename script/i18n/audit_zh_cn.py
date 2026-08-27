#!/usr/bin/env python3
"""Audit every statically identifiable production UI string for zh-CN."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
CATALOG_PATH = REPOSITORY_ROOT / "crates/warp_i18n/locales/zh-CN.tsv"
EXCLUSIONS_PATH = REPOSITORY_ROOT / "script/i18n/zh_cn_exclusions.json"

SOURCE_ROOTS = (
    REPOSITORY_ROOT / "app/src",
    REPOSITORY_ROOT / "crates/onboarding/src",
    REPOSITORY_ROOT / "crates/ui_components/src",
    REPOSITORY_ROOT / "crates/warpui_core/src",
    REPOSITORY_ROOT / "crates/warpui/src",
    REPOSITORY_ROOT / "crates/editor/src",
    REPOSITORY_ROOT / "crates/warp_tui/src",
)

# These are UI boundaries whose first argument is rendered for a person. Keep
# this list narrow and explicit: adding a sink expands the production contract.
SINK = r"""(?:
    (?:Text|TuiText|WrappableText|Span|Paragraph)::(?:new|new_inline)
  | FormattedTextElement::from_str
  | FormattedTextFragment::(?:
        plain_text
      | weighted
      | bold
      | italic
      | bold_italic
      | strikethrough
      | underline
      | inline_code
      | hyperlink
      | hyperlink_action
    )
  | CustomMenuItem::new(?:_with_submenu)?
  | Menu::new
  | (?:warp_i18n::)?localize_(?:ui|static|ref)
  | \.(?:
        with_text_label
      | with_centered_text_label
      | with_tooltip
      | tool_tip
      | with_hint_text
      | with_description
      | with_subtitle
      | with_header
      | with_detail
      | with_help_message
      | with_badge
      | with_right_side_label
      | with_command_description
      | set_placeholder_text
      | set_label
      | set_title
      | set_error
      | set_browsing_error
      | set_menu_header_to_static
      | span
      | paragraph
      | wrappable_text
      | label
    )
)"""

RUST_STRING = r'''(?:
    r(?P<hashes>\#{0,16})"(?P<raw>.*?)"(?P=hashes)
  | "(?P<cooked>(?:[^"\\]|\\.)*)"
)'''

STATIC_CALL = re.compile(
    rf"(?P<sink>{SINK})\s*\(\s*(?:&\s*)?(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
FORMATTED_CALL = re.compile(
    rf"(?P<sink>{SINK})\s*\(\s*format!\s*\(\s*(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
LOCALIZED_TEMPLATE = re.compile(
    rf"(?:warp_i18n::)?localize_format!\s*\(\s*(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
CONST_DEFINITION = re.compile(
    rf"\b(?:pub(?:\([^)]*\))?\s+)?const\s+"
    rf"(?P<name>[A-Z][A-Z0-9_]*)\s*:\s*&(?:'static\s+)?str\s*=\s*"
    rf"(?:&\s*)?(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
CONST_CALL = re.compile(
    rf"(?P<sink>{SINK})\s*\(\s*(?:&\s*)?(?P<name>[A-Z][A-Z0-9_]*)\b",
    re.DOTALL | re.VERBOSE,
)
TEXT_AND_ICON_STATIC = re.compile(
    rf"TextAndIcon::new\s*\(\s*[^,]+,\s*(?:&\s*)?(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
TEXT_AND_ICON_FORMATTED = re.compile(
    rf"TextAndIcon::new\s*\(\s*[^,]+,\s*format!\s*\(\s*(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
NAMED_PLACEHOLDER = re.compile(r"(?<!\{)\{([A-Za-z_][A-Za-z0-9_]*)\}(?!\})")
EXPLICIT_LOCALIZATION_SINKS = {
    "set_label",
    "set_title",
    "set_error",
    "set_browsing_error",
    "with_header",
    "with_detail",
    "with_help_message",
    "with_description",
    "with_badge",
    "with_right_side_label",
    "with_command_description",
}
TUI_SPANS_START = re.compile(r"TuiText::from_spans\s*\(\s*\[")
TUI_STATIC_SPAN = re.compile(
    rf"\(\s*(?P<literal>{RUST_STRING})(?:\.(?:to_owned|to_string)\(\))?\s*,",
    re.DOTALL | re.VERBOSE,
)
TUI_FORMATTED_SPAN = re.compile(
    rf"\(\s*format!\s*\(\s*(?P<literal>{RUST_STRING})",
    re.DOTALL | re.VERBOSE,
)
TUI_CONST_SPAN = re.compile(
    r"\(\s*(?P<name>[A-Z][A-Z0-9_]*)(?:\.(?:to_owned|to_string)\(\))?\s*,",
    re.DOTALL | re.VERBOSE,
)


class CatalogError(ValueError):
    """Raised when a localization artifact violates its contract."""


@dataclass(frozen=True, order=True)
class Occurrence:
    path: str
    line: int
    sink: str
    source: str

    @property
    def exclusion_key(self) -> tuple[str, str, str]:
        return (self.path, self.sink, self.source)

    def as_dict(self) -> dict[str, object]:
        return {
            "path": self.path,
            "line": self.line,
            "sink": self.sink,
            "source": self.source,
        }


def load_catalog(path: Path) -> dict[str, str]:
    catalog: dict[str, str] = {}
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw_line or (raw_line.startswith("#") and "\t" not in raw_line):
            continue
        if raw_line.count("\t") != 1:
            raise CatalogError(
                f"{path}:{line_number}: expected exactly one tab separator"
            )
        encoded_source, encoded_translation = raw_line.split("\t")
        source = decode_catalog_field(encoded_source, path, line_number)
        translation = decode_catalog_field(encoded_translation, path, line_number)
        if not source or not translation:
            raise CatalogError(f"{path}:{line_number}: source and translation are required")
        if source in catalog:
            raise CatalogError(f"{path}:{line_number}: duplicate source {source!r}")
        catalog[source] = translation
    return catalog


def decode_catalog_field(value: str, path: Path, line_number: int) -> str:
    output: list[str] = []
    index = 0
    replacements = {"n": "\n", "r": "\r", "t": "\t", "\\": "\\"}
    while index < len(value):
        if value[index] != "\\":
            output.append(value[index])
            index += 1
            continue
        if index + 1 >= len(value) or value[index + 1] not in replacements:
            invalid = value[index : index + 2]
            raise CatalogError(f"{path}:{line_number}: invalid escape {invalid!r}")
        output.append(replacements[value[index + 1]])
        index += 2
    return "".join(output)


def load_exclusions(
    path: Path,
) -> tuple[dict[tuple[str, str, str], str], dict[str, str]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}, {}
    except json.JSONDecodeError as error:
        raise CatalogError(f"{path}:{error.lineno}: invalid JSON: {error.msg}") from error

    if (
        document.get("version") != 1
        or not isinstance(document.get("sources"), list)
        or not isinstance(document.get("occurrences"), list)
    ):
        raise CatalogError(
            f"{path}: expected version 1 plus sources and occurrences arrays"
        )

    exclusions: dict[tuple[str, str, str], str] = {}
    for index, entry in enumerate(document["occurrences"], 1):
        if not isinstance(entry, dict):
            raise CatalogError(f"{path}: occurrence {index} must be an object")
        values = tuple(entry.get(field) for field in ("path", "sink", "source"))
        reason = entry.get("reason")
        if not all(isinstance(value, str) and value for value in (*values, reason)):
            raise CatalogError(
                f"{path}: occurrence {index} requires non-empty path, sink, source, and reason"
            )
        if values in exclusions:
            raise CatalogError(f"{path}: duplicate exclusion at occurrence {index}")
        exclusions[values] = reason

    source_exclusions: dict[str, str] = {}
    for index, entry in enumerate(document["sources"], 1):
        if not isinstance(entry, dict):
            raise CatalogError(f"{path}: source {index} must be an object")
        source = entry.get("source")
        reason = entry.get("reason")
        if not isinstance(source, str) or not isinstance(reason, str) or not reason:
            raise CatalogError(
                f"{path}: source {index} requires a string source and non-empty reason"
            )
        if source in source_exclusions:
            raise CatalogError(f"{path}: duplicate source exclusion at source {index}")
        source_exclusions[source] = reason
    return exclusions, source_exclusions


def is_production_rust_source(path: Path) -> bool:
    lowered_parts = {part.lower() for part in path.parts}
    stem = path.stem.lower()
    return (
        path.suffix == ".rs"
        and "tests" not in lowered_parts
        and "examples" not in lowered_parts
        and not stem.endswith("_test")
        and not stem.endswith("_tests")
        and stem not in {"test", "tests"}
    )


def mask_rust_comments(contents: str) -> str:
    """Replace Rust comments with spaces while retaining byte/line offsets."""

    chars = list(contents)
    index = 0
    length = len(chars)
    while index < length:
        if contents.startswith("//", index):
            end = contents.find("\n", index)
            end = length if end == -1 else end
            for position in range(index, end):
                chars[position] = " "
            index = end
            continue
        if contents.startswith("/*", index):
            depth = 1
            end = index + 2
            while end < length and depth:
                if contents.startswith("/*", end):
                    depth += 1
                    end += 2
                elif contents.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            for position in range(index, end):
                if chars[position] != "\n":
                    chars[position] = " "
            index = end
            continue
        if chars[index] == '"':
            index += 1
            while index < length:
                if chars[index] == "\\":
                    index += 2
                elif chars[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            continue
        if chars[index] == "r":
            raw_match = re.match(r'r(?P<hashes>\#{0,16})"', contents[index:])
            if raw_match:
                delimiter = '"' + raw_match.group("hashes")
                end = contents.find(delimiter, index + raw_match.end())
                index = length if end == -1 else end + len(delimiter)
                continue
        if chars[index] == "'":
            index += 1
            while index < length:
                if chars[index] == "\\":
                    index += 2
                elif chars[index] == "'":
                    index += 1
                    break
                else:
                    index += 1
            continue
        index += 1
    return "".join(chars)


def decode_rust_string(match: re.Match[str]) -> str:
    raw = match.groupdict().get("raw")
    if raw is not None:
        return raw

    body = match.group("cooked")
    output: list[str] = []
    index = 0
    while index < len(body):
        if body[index] != "\\":
            output.append(body[index])
            index += 1
            continue
        index += 1
        if index >= len(body):
            raise CatalogError("unterminated Rust string escape")
        escaped = body[index]
        if escaped in "\r\n":
            if escaped == "\r" and index + 1 < len(body) and body[index + 1] == "\n":
                index += 1
            index += 1
            while index < len(body) and body[index] in " \t\r\n":
                index += 1
            continue
        replacements = {
            "n": "\n",
            "r": "\r",
            "t": "\t",
            "0": "\0",
            "\\": "\\",
            '"': '"',
            "'": "'",
        }
        if escaped in replacements:
            output.append(replacements[escaped])
            index += 1
            continue
        if escaped == "x" and index + 2 < len(body):
            output.append(chr(int(body[index + 1 : index + 3], 16)))
            index += 3
            continue
        if escaped == "u" and index + 1 < len(body) and body[index + 1] == "{":
            end = body.find("}", index + 2)
            if end == -1:
                raise CatalogError("unterminated Rust Unicode escape")
            output.append(chr(int(body[index + 2 : end].replace("_", ""), 16)))
            index = end + 1
            continue
        raise CatalogError(f"unsupported Rust string escape \\{escaped}")
    return "".join(output)


def display_sink(sink: str) -> str:
    return sink[1:] if sink.startswith(".") else sink


def matching_square_bracket(contents: str, start: int) -> int | None:
    depth = 0
    index = start
    while index < len(contents):
        character = contents[index]
        if character == '"':
            index += 1
            while index < len(contents):
                if contents[index] == "\\":
                    index += 2
                elif contents[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            continue
        if character == "r":
            raw_match = re.match(r'r(?P<hashes>\#{0,16})"', contents[index:])
            if raw_match:
                delimiter = '"' + raw_match.group("hashes")
                end = contents.find(delimiter, index + raw_match.end())
                index = len(contents) if end == -1 else end + len(delimiter)
                continue
        if character == "[":
            depth += 1
        elif character == "]":
            depth -= 1
            if depth == 0:
                return index
        index += 1
    return None


def extract_tui_span_occurrences(
    contents: str,
    searchable: str,
    relative_path: str,
    constants: dict[str, str],
) -> tuple[list[Occurrence], list[Occurrence]]:
    static: list[Occurrence] = []
    formatted: list[Occurrence] = []
    for start_match in TUI_SPANS_START.finditer(searchable):
        opening_bracket = searchable.find("[", start_match.start(), start_match.end())
        closing_bracket = matching_square_bracket(searchable, opening_bracket)
        if closing_bracket is None:
            raise CatalogError(
                f"{relative_path}:{contents.count(chr(10), 0, opening_bracket) + 1}: "
                "unterminated TuiText::from_spans array"
            )
        body_start = opening_bracket + 1
        body = searchable[body_start:closing_bracket]
        for pattern, destination in (
            (TUI_STATIC_SPAN, static),
            (TUI_FORMATTED_SPAN, formatted),
        ):
            for match in pattern.finditer(body):
                if pattern is TUI_STATIC_SPAN and re.search(
                    r"format!\s*$", body[max(0, match.start() - 16) : match.start()]
                ):
                    continue
                absolute_start = body_start + match.start()
                destination.append(
                    Occurrence(
                        path=relative_path,
                        line=contents.count("\n", 0, absolute_start) + 1,
                        sink="TuiText::from_spans",
                        source=decode_rust_string(match),
                    )
                )
        for match in TUI_CONST_SPAN.finditer(body):
            source = constants.get(match.group("name"))
            if source is None:
                continue
            absolute_start = body_start + match.start()
            static.append(
                Occurrence(
                    path=relative_path,
                    line=contents.count("\n", 0, absolute_start) + 1,
                    sink="TuiText::from_spans",
                    source=source,
                )
            )
    return static, formatted


def extract_occurrences() -> tuple[list[Occurrence], list[Occurrence], list[Occurrence]]:
    static: list[Occurrence] = []
    formatted: list[Occurrence] = []
    localized_templates: list[Occurrence] = []
    production_sources: list[tuple[str, str, str]] = []
    for source_root in SOURCE_ROOTS:
        for path in sorted(source_root.rglob("*.rs")):
            if not is_production_rust_source(path):
                continue
            contents = path.read_text(encoding="utf-8")
            searchable = mask_rust_comments(contents)
            relative_path = path.relative_to(REPOSITORY_ROOT).as_posix()
            production_sources.append((relative_path, contents, searchable))

    global_constant_values: dict[str, set[str]] = {}
    for _, _, searchable in production_sources:
        for match in CONST_DEFINITION.finditer(searchable):
            global_constant_values.setdefault(match.group("name"), set()).add(
                decode_rust_string(match)
            )
    unique_global_constants = {
        name: next(iter(values))
        for name, values in global_constant_values.items()
        if len(values) == 1
    }

    for relative_path, contents, searchable in production_sources:
            local_constants = {
                match.group("name"): decode_rust_string(match)
                for match in CONST_DEFINITION.finditer(searchable)
            }
            resolvable_constants = unique_global_constants | local_constants
            tui_static, tui_formatted = extract_tui_span_occurrences(
                contents, searchable, relative_path, resolvable_constants
            )
            static.extend(tui_static)
            formatted.extend(tui_formatted)
            for match in CONST_CALL.finditer(searchable):
                source = resolvable_constants.get(match.group("name"))
                if source is None:
                    continue
                static.append(
                    Occurrence(
                        path=relative_path,
                        line=contents.count("\n", 0, match.start()) + 1,
                        sink=display_sink(match.group("sink")),
                        source=source,
                    )
                )
            for pattern, destination in (
                (TEXT_AND_ICON_STATIC, static),
                (TEXT_AND_ICON_FORMATTED, formatted),
            ):
                for match in pattern.finditer(searchable):
                    destination.append(
                        Occurrence(
                            path=relative_path,
                            line=contents.count("\n", 0, match.start()) + 1,
                            sink="TextAndIcon::new",
                            source=decode_rust_string(match),
                        )
                    )
            for pattern, destination in (
                (STATIC_CALL, static),
                (FORMATTED_CALL, formatted),
                (LOCALIZED_TEMPLATE, localized_templates),
            ):
                for match in pattern.finditer(searchable):
                    destination.append(
                        Occurrence(
                            path=relative_path,
                            line=contents.count("\n", 0, match.start()) + 1,
                            sink=(
                                "localize_format"
                                if pattern is LOCALIZED_TEMPLATE
                                else display_sink(match.group("sink"))
                            ),
                            source=decode_rust_string(match),
                        )
                    )
    return static, formatted, localized_templates


def audit() -> dict[str, object]:
    catalog = load_catalog(CATALOG_PATH)
    exclusions, source_exclusions = load_exclusions(EXCLUSIONS_PATH)
    static, formatted, localized_templates = extract_occurrences()
    all_occurrences = static + formatted
    occurrence_keys = {occurrence.exclusion_key for occurrence in all_occurrences}
    occurrence_sources = {occurrence.source for occurrence in all_occurrences}
    stale_exclusions = [
        {"path": key[0], "sink": key[1], "source": key[2], "reason": reason}
        for key, reason in exclusions.items()
        if key not in occurrence_keys
    ]
    stale_exclusions.sort(key=lambda entry: (entry["path"], entry["sink"], entry["source"]))
    stale_exclusions.extend(
        {"path": "*", "sink": "*", "source": source, "reason": reason}
        for source, reason in source_exclusions.items()
        if source not in occurrence_sources
    )

    def is_excluded(occurrence: Occurrence) -> bool:
        return (
            occurrence.exclusion_key in exclusions
            or occurrence.source in source_exclusions
        )

    required_static = [
        occurrence for occurrence in static if not is_excluded(occurrence)
    ]
    required_sources = {occurrence.source for occurrence in required_static}
    required_sources.update(occurrence.source for occurrence in localized_templates)
    missing_sources = sorted(required_sources.difference(catalog))
    covered_sources = required_sources.intersection(catalog)
    coverage = len(covered_sources) / len(required_sources) if required_sources else 1.0
    placeholder_mismatches = []
    for source, translation in catalog.items():
        source_placeholders = sorted(set(NAMED_PLACEHOLDER.findall(source)))
        translation_placeholders = sorted(set(NAMED_PLACEHOLDER.findall(translation)))
        if source_placeholders != translation_placeholders:
            placeholder_mismatches.append(
                {
                    "source": source,
                    "translation": translation,
                    "source_placeholders": source_placeholders,
                    "translation_placeholders": translation_placeholders,
                }
            )

    unadapted_formatted = [
        occurrence.as_dict()
        for occurrence in formatted
        if not is_excluded(occurrence)
    ]
    missing_occurrences = [
        occurrence.as_dict()
        for occurrence in required_static + localized_templates
        if occurrence.source not in catalog
    ]
    unadapted_static = [
        occurrence.as_dict()
        for occurrence in static
        if not is_excluded(occurrence)
        and (
            occurrence.sink in {"TuiText::new", "TuiText::from_spans"}
            or occurrence.sink.startswith("FormattedTextFragment::")
            or occurrence.sink in EXPLICIT_LOCALIZATION_SINKS
        )
    ]

    return {
        "catalog_entries": len(catalog),
        "production_files": len(
            {occurrence.path for occurrence in static + formatted + localized_templates}
        ),
        "static_occurrences": len(static),
        "required_sources": len(required_sources),
        "covered_sources": len(covered_sources),
        "excluded_occurrences": sum(
            is_excluded(occurrence) for occurrence in all_occurrences
        ),
        "localized_templates": len(localized_templates),
        "coverage": coverage,
        "missing_sources": missing_sources,
        "missing_occurrences": missing_occurrences,
        "unadapted_formatted_occurrences": unadapted_formatted,
        "unadapted_static_occurrences": unadapted_static,
        "stale_exclusions": stale_exclusions,
        "placeholder_mismatches": placeholder_mismatches,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="require 100% static coverage and no unclassified formatted UI copy",
    )
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        result = audit()
    except (CatalogError, OSError, UnicodeError, ValueError) as error:
        print(f"localization audit failed: {error}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(
            "zh-CN localization: "
            f"{result['covered_sources']}/{result['required_sources']} "
            f"production UI sources covered ({result['coverage']:.1%}); "
            f"{result['catalog_entries']} catalog entries; "
            f"{result['excluded_occurrences']} explicitly excluded occurrences"
        )
        if result["missing_occurrences"]:
            print("Missing static UI translations:")
            for occurrence in result["missing_occurrences"]:
                print(
                    f"  {occurrence['path']}:{occurrence['line']} "
                    f"[{occurrence['sink']}] {occurrence['source']!r}"
                )
        if result["unadapted_formatted_occurrences"]:
            print("Formatted UI occurrences requiring localization or an exclusion:")
            for occurrence in result["unadapted_formatted_occurrences"]:
                print(
                    f"  {occurrence['path']}:{occurrence['line']} "
                    f"[{occurrence['sink']}] {occurrence['source']!r}"
                )
        if result["unadapted_static_occurrences"]:
            print("Static UI occurrences requiring an explicit localization wrapper:")
            for occurrence in result["unadapted_static_occurrences"]:
                print(
                    f"  {occurrence['path']}:{occurrence['line']} "
                    f"[{occurrence['sink']}] {occurrence['source']!r}"
                )
        if result["stale_exclusions"]:
            print("Stale exclusions:")
            for exclusion in result["stale_exclusions"]:
                print(
                    f"  {exclusion['path']} [{exclusion['sink']}] "
                    f"{exclusion['source']!r}: {exclusion['reason']}"
                )
        if result["placeholder_mismatches"]:
            print("Catalog placeholder mismatches:")
            for mismatch in result["placeholder_mismatches"]:
                print(
                    f"  {mismatch['source']!r}: "
                    f"{mismatch['source_placeholders']} != "
                    f"{mismatch['translation_placeholders']}"
                )

    check_failed = (
        result["coverage"] != 1.0
        or bool(result["unadapted_formatted_occurrences"])
        or bool(result["unadapted_static_occurrences"])
        or bool(result["stale_exclusions"])
        or bool(result["placeholder_mismatches"])
    )
    if args.check and check_failed:
        print(
            "zh-CN requires 100% coverage, zero unclassified formatted UI occurrences, "
            "and zero stale exclusions",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
