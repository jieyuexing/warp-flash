use markdown_parser::FormattedTextLine;

use super::*;

#[test]
fn parses_structures_from_the_entire_terminal_snapshot() {
    let source = "# Result\n\n• First\n• Second\n\n| A | B |\n| - | - |\n| 1 | 2 |";

    let parsed = parse_cli_agent_markdown_snapshot(source).expect("snapshot should parse");

    assert!(
        parsed
            .lines
            .iter()
            .any(|line| matches!(line, FormattedTextLine::Heading(_)))
    );
    assert_eq!(
        parsed
            .lines
            .iter()
            .filter(|line| matches!(line, FormattedTextLine::UnorderedList(_)))
            .count(),
        2
    );
    assert!(
        parsed
            .lines
            .iter()
            .any(|line| matches!(line, FormattedTextLine::Table(_)))
    );
}

#[test]
fn normalizes_visual_horizontal_rules_without_rewriting_table_separators() {
    let source = "Before\n━━━━━━━━\nKey      Value\n━━━━━━━  ━━━━━━━\nalpha    beta";

    let normalized = normalize_terminal_visual_markdown(source).expect("snapshot should normalize");

    assert_eq!(
        normalized,
        "Before\n---\nKey      Value\n━━━━━━━  ━━━━━━━\nalpha    beta"
    );
}

#[test]
fn projects_blockquotes_as_visual_quote_lines() {
    let source = concat!(
        "\u{2022} \u{53ef}\u{4ee5}\u{8fd9}\u{6837}\u{56de}\u{590d}\u{ff1a}\n",
        "\n",
        "> \u{5bf9}\u{ff0c}\u{8fd9}\u{4e2a}\u{4e1a}\u{52a1}\u{6821}\u{9a8c}\u{8981}\u{653e}\u{5230} **SQL4** \u{524d}\u{9762}\u{3002}\n",
        ">\n",
        "> SQL4 \u{524d}\u{5148}\u{7528} REMAIN \u{6821}\u{9a8c}\u{ff1a}\n",
        ">\n",
        "> - CTN_NUM = 1: QTY = REMAIN\n",
        "> - CTN_NUM > 1: REMAIN > QTY \u{00d7} (CTN_NUM - 1)\n",
        "> > nested quote\n",
    );

    let normalized = normalize_terminal_visual_markdown(source).expect("snapshot should normalize");

    assert_eq!(
        normalized,
        concat!(
            "- \u{53ef}\u{4ee5}\u{8fd9}\u{6837}\u{56de}\u{590d}\u{ff1a}\n",
            "\n",
            "\u{2502} \u{5bf9}\u{ff0c}\u{8fd9}\u{4e2a}\u{4e1a}\u{52a1}\u{6821}\u{9a8c}\u{8981}\u{653e}\u{5230} **SQL4** \u{524d}\u{9762}\u{3002}\n",
            "\u{2502}\n",
            "\u{2502} SQL4 \u{524d}\u{5148}\u{7528} REMAIN \u{6821}\u{9a8c}\u{ff1a}\n",
            "\u{2502}\n",
            "\u{2502}   \u{2022} CTN_NUM = 1: QTY = REMAIN\n",
            "\u{2502}   \u{2022} CTN_NUM > 1: REMAIN > QTY \u{00d7} (CTN_NUM - 1)\n",
            "\u{2502} \u{2502} nested quote",
        )
    );

    let parsed = parse_cli_agent_markdown_snapshot(source).expect("snapshot should parse");
    assert!(
        parsed
            .raw_text()
            .contains("\u{2502} \u{5bf9}\u{ff0c}\u{8fd9}\u{4e2a}\u{4e1a}\u{52a1}\u{6821}\u{9a8c}")
    );
    assert!(
        parsed
            .raw_text()
            .contains("\u{2502}   \u{2022} CTN_NUM = 1")
    );
    assert!(!parsed.raw_text().lines().any(|line| line.starts_with('>')));
}

#[test]
fn rejects_empty_and_oversized_snapshots() {
    assert!(parse_cli_agent_markdown_snapshot("  \n").is_none());

    let oversized = "a".repeat(MAX_SNAPSHOT_BYTES + 1);
    assert!(parse_cli_agent_markdown_snapshot(&oversized).is_none());
}

#[test]
fn snapshot_source_includes_complete_inline_cli_block() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block(
        "codex",
        "# First\r\nline 2\r\nline 3\r\nline 4\r\nline 5\r\nline 6\r\nline 7\r\nline 8\r\nline 9\r\nline 10\r\nline 11\r\nLast",
    );

    let source = cli_agent_markdown_snapshot_source(&model, true)
        .expect("inline CLI output should be available");

    assert_eq!(
        source,
        "# First\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nLast"
    );
}

#[test]
fn snapshot_source_requires_an_active_cli_agent_session() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("codex", "# Result");

    assert!(cli_agent_markdown_snapshot_source(&model, false).is_none());
}

#[test]
fn snapshot_source_rejects_alternate_screen_frames() {
    let mut model = TerminalModel::mock(None, None);
    model.simulate_long_running_block("codex", "# Result");
    model.enter_alt_screen(true);

    assert!(cli_agent_markdown_snapshot_source(&model, true).is_none());
}
