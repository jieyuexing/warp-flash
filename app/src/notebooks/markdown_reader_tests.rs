use super::*;

#[test]
fn authoritative_documents_retain_exact_source_and_source_actions() {
    let source = "# Heading\r\n\r\n> Quote\r\n";
    let document = MarkdownReaderDocument::authoritative(source);

    assert_eq!(document.source(), source);
    assert_eq!(
        document.provenance(),
        MarkdownDocumentProvenance::AuthoritativeSource
    );
    assert!(document.capabilities().source_mode);
    assert!(document.capabilities().copy_source);
    assert!(document.capabilities().rendered_selection);
}

#[test]
fn visual_snapshots_never_claim_source_capabilities() {
    let document = MarkdownReaderDocument::visual_snapshot("# Result\n\nRendered output");

    assert_eq!(
        document.provenance(),
        MarkdownDocumentProvenance::VisualSnapshot
    );
    assert!(!document.capabilities().source_mode);
    assert!(!document.capabilities().copy_source);
    assert!(document.capabilities().rendered_selection);
}

#[test]
fn reading_projection_adds_quote_rails_and_preserves_nested_structure_cues() {
    let source = concat!(
        "> plain **text**\n",
        "> ## heading\n",
        "> - list item\n",
        "> > nested\n",
    );

    assert_eq!(
        project_markdown_for_reading(source),
        concat!(
            "│ plain **text**\n",
            "## │ heading\n",
            "- │ list item\n",
            "│ │ nested\n",
        )
    );
}

#[test]
fn reading_projection_does_not_rewrite_fenced_code() {
    let source = "```md\n> literal\n```\n\n> rendered";

    assert_eq!(
        project_markdown_for_reading(source),
        "```md\n> literal\n```\n\n│ rendered"
    );
}
