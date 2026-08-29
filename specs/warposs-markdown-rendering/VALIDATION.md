# Markdown reader validation record

## 2026-08-29 desktop foundation

Renderer commit: `9d6c3ca1a`

Fixture commit: `fa223419c`

Environment: macOS, debug `warp-oss` bundle, light and dark Warp themes

Isolation:

- Bundle: `/Users/jieyuexing/Applications/Warposs Markdown QA 20260829.app`
- Bundle identifier: `dev.warposs.MarkdownQA20260829`
- Data profile: `markdown-qa-20260829`
- Config: `/Users/jieyuexing/.warp-oss-markdown-qa-20260829`
- The production `/Applications/Warposs.app` process and terminal-server child remained running
  while the isolated QA process and its own terminal-server child were used.

Visual fixture:
[`fixtures/obsidian-reader-reference.md`](fixtures/obsidian-reader-reference.md)

### Real-display results

| Check | Result | Evidence boundary |
| --- | --- | --- |
| Reading/Source segmented control | Pass | Both directions rendered; returning to Reading restored an approximately equivalent scroll fraction |
| Readable line length | Pass | Reading content remained centered and bounded in a maximized pane; narrow panes reflowed the same document |
| H1-H6 hierarchy | Pass | Six distinct size/weight levels were visible in Reading view |
| Inline styles and links | Pass | Bold, italic, strikethrough, inline code, and themed link treatment were visible |
| Blockquote compatibility projection | Pass | Single and nested visual rails were visible; this is not structural blockquote IR |
| Lists and tasks | Pass | Nested unordered, ordered, open-task, and completed-task rows remained distinct |
| Fenced code | Pass | Themed surface, border, syntax colors, language label, and code-copy affordance were visible |
| Wide table | Pass | Header/row styling and an internal horizontal scrollbar remained available inside the reading pane |
| Pointer selection | Pass | Dragging across rendered prose produced a visible selection without switching to Source mode |
| Theme adaptation | Pass | Light and dark themes updated text, surfaces, borders, code, tables, and selection without a stale light-only surface |

Clipboard contents were not mutated during real-display QA. Exact Copy Source and rendered-reader
Copy routing are covered by focused unit tests instead.

### Automated results

- `cargo check -p warp --lib`: pass
- `cargo test -p warp --lib cli_agent_markdown_reader -- --nocapture`: 8 passed
- `cargo test -p warp --lib notebooks::file::tests -- --nocapture`: 10 passed
- `cargo test -p warp --lib notebooks::markdown_reader::tests -- --nocapture`: 5 passed
- `cargo test -p warp_editor heading_typography_is_scoped_to_rich_text_styles -- --nocapture`:
  1 passed
- `./script/format`: pass
- `git diff --check`: pass

### Not claimed by this record

- An external Agent CLI runtime walkthrough; the shared reader is covered visually through the
  file fixture, while CLI eligibility, provenance, context-menu, and copy routing are unit-tested.
- Windows or Linux GUI behavior. The branch was local-only at validation time and was not pushed
  merely to make it reachable by a cloud runner.
- CommonMark/GFM conformance or structural blockquotes; those remain V2 parser/IR work.
- Live Preview, WYSIWYG editing, folding, outline navigation, or proprietary Obsidian behavior.
- Cross-platform pixel identity. The acceptance target is shared hierarchy and interaction with
  adaptive platform font rasterization and theme rendering.
