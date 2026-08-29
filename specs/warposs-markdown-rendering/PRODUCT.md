# Warposs Markdown rendering v2 product requirements document

Status: `foundation landed`; research and product contract complete; file-backed Markdown and
bounded external CLI visual snapshots now share one selectable desktop reader and one scoped
reading style profile. The source-preserving V2 parser/IR and transcript/TUI rollout have not
started.

Product owner: Warposs OSS

Repository evidence baseline: Warp `8313e69fc01d99c32e0b4f9ee24cdec6b3bbb214`, observed
2026-08-27

Implementation snapshot: Warp `72bc599c7`, observed 2026-08-29

External research snapshot: default branches observed 2026-08-27; exact commits are recorded in
[GitHub project research](#github-project-research)

Obsidian documentation snapshot: official Help and Developer Documentation observed 2026-08-29

## Executive decision

Warposs should treat Markdown as a source-backed semantic document, not as a collection of
independently recognized lines. P0 will provide one CommonMark 0.31.2 plus GitHub Flavored
Markdown compatibility contract, one source-preserving intermediate representation, and
surface-specific presenters for the desktop GUI and headless TUI.

The implementation direction is:

1. adopt `pulldown-cmark` behind a Warposs-owned adapter for standards parsing and byte-range
   source mapping;
2. retain Warposs-owned semantic nodes and existing specialized views for code, tables, images,
   Mermaid, and Warp fenced extensions;
3. keep a stable rendered prefix and reparse only the mutable document tail while an agent
   response is streaming;
4. expose Reading and Source modes so malformed or unsupported input is always recoverable; and
5. roll out behind one high-level `WarpossMarkdownRenderingV2` feature flag with dual-parse
   comparison before replacing the current parser path.

This is not a proposal to embed a webview or import a third-party renderer wholesale. Obsidian,
Glamour, Codex, Gemini CLI, mdcat, and Ratatui Markdown projects are design evidence; their render
trees, interaction models, and runtimes do not match WarpUI directly.

## Evidence classification

This document uses three evidence classes:

- **Verified** means inspected in the repository baseline or an exact upstream commit.
- **Inferred** means a conclusion drawn from verified source structure, but not proven by a
  current runtime walkthrough.
- **Proposed** means a product requirement or implementation direction in this PRD.

No runtime acceptance is claimed by this document.

### Verified Warposs baseline

| Area | Current evidence | Consequence |
| --- | --- | --- |
| Shared parser | [`markdown_parser`](../../crates/markdown_parser/src/markdown_parser.rs) is a custom `nom` parser. `parse_markdown` and `parse_markdown_with_gfm_tables` are separate entry points. | Tables are not part of one uniform dialect contract. |
| Semantic model | [`FormattedTextLine`](../../crates/markdown_parser/src/lib.rs) has heading, paragraph line, ordered/unordered/task list, code, rule, embedded object, image, and table variants. It has no structural blockquote, footnote, alert, or raw-HTML node. | Unsupported structures can be flattened before a presenter sees them. |
| Agent segmentation | [`parse_markdown_into_text_and_code_sections`](../../app/src/ai/agent/util.rs) recognizes code, GFM tables, block images, and Mermaid before plain text is parsed. | A new parser must preserve these specialized blocks without creating two competing document identities. |
| GUI | The Agent block renderer already has rich code, structured table, image, and Mermaid views in [`common.rs`](../../app/src/ai/blocklist/block/view_impl/common.rs). | P0 is an evolution of existing views, not a greenfield renderer. |
| TUI | [`tui_markdown.rs`](../../crates/warp_tui/src/tui_markdown.rs) renders shared semantic lines, including responsive tables and text fallbacks. All heading levels currently share one style, links append their destination as text, and unsupported embeds use a fallback label. | TUI has a viable presentation layer but lacks full semantics and interactive link metadata. |
| Large code | [`tui_code_block_view.rs`](../../crates/warp_tui/src/tui_code_block_view.rs) bounds syntax highlighting at 256 KiB or 5,000 lines and falls back safely. | V2 must retain explicit resource ceilings rather than making rich rendering unbounded. |
| Existing extensions | The parser recognizes `warp-embedded-object`, `warp-runnable-command`, and `warp-markdown-table` fenced languages. | Parser migration requires byte-for-byte source compatibility for these extensions. |
| Existing feature work | Markdown tables, images, Mermaid, and editable Mermaid already have feature flags and focused specs, including [`mermaid-markdown-in-plans`](../mermaid-markdown-in-plans/PRODUCT.md). | V2 owns the common contract and composition; it does not reopen those product decisions. |
| Shared desktop reader | [`MarkdownReaderDocument` and `MarkdownReaderView`](../../app/src/notebooks/markdown_reader.rs) now separate exact authoritative source from reading projection, declare source/copy/selection capabilities, and host one selectable rich-text view used by both file Markdown and the external CLI reader. | The two inputs retain different provenance but no longer maintain separate desktop reading renderers. This is the migration seam for the future source-preserving IR. |
| Inline CLI block output | [`Block::output_to_string_force_full_grid_contents`](../../app/src/terminal/model/block.rs) reads the active block's complete retained output grid, including flat-storage scrollback, while respecting visual secret obfuscation and terminal soft-wrap metadata. | Warposs can build a complete visual snapshot for an inline Agent CLI tab without clipboard automation or OCR. It is rendered terminal text, not authoritative Markdown source. |
| Alternate-screen visual buffer | [`TerminalModel`](../../app/src/terminal/model/terminal_model.rs) constructs `AltScreen` with a zero scroll limit, and [`GridHandler::scroll_region_up`](../../crates/warp_terminal/src/model/grid/ansi_handler.rs) deliberately does not move alternate-screen rows into flat-storage scrollback. | An alternate-screen frame is only the current viewport, so it cannot honestly power a whole-tab reader. The action remains unavailable in this mode. |
| External CLI events | [`CLIAgentEventPayload`](../../app/src/terminal/cli_agent_sessions/event/mod.rs) accepts an optional `response` over structured OSC 777, and the session model retains it for a completed turn. Plain PTY cells and Codex's OSC 9 fallback do not provide authoritative Markdown source. | Structured transport remains the future lossless path, but it is not required for a best-effort whole-tab visual reader. |
| Codex plugin payload | At [`warpdotdev/codex-warp@31ce59d`](https://github.com/warpdotdev/codex-warp/blob/31ce59d9011cfb1d78f265649a228dac5de58d76/plugins/warp/scripts/on-stop.sh), the Stop hook reads `last_assistant_message` but truncates it to 200 characters before emitting `response`. It also supplies a `transcript_path`. | The event cannot support a complete semantic response today. A visual snapshot avoids that length limit, while the event's path remains untrusted and must never grant arbitrary local-file reads. |

### Inferred product gap

The current stack can render many common AI answers well, but correctness depends on which
surface segmented the text and which parser entry point it used. A blockquote, nested mixed
block, reference definition, or incomplete streaming fence can lose structure before the GUI or
TUI gets a chance to present it. Static source inspection also shows no single invariant tying
all chunks with the same agent message ID to one Markdown document.

The P0 problem is therefore semantic consistency and streaming stability, not simply adding
more colors or syntax rules.

## Problem statement

Developers read long, structured AI output while it is still arriving. That content frequently
contains code, tables, links to local files, task lists, quotes, and diagrams. Today the user can
encounter four classes of failure:

1. valid CommonMark/GFM is rendered differently across desktop and TUI;
2. an incomplete construct changes earlier layout or flips prose into code during streaming;
3. a narrow viewport makes tables, links, or nested lists unreadable; and
4. unsupported or unsafe content disappears, looks executable, or cannot be recovered exactly.

These failures damage trust: the user must switch tools or inspect raw output to determine what
the model actually said.

## Users and jobs

### Developer following an agent response

The developer can scan hierarchy, distinguish prose from code, open a trusted link, copy the
exact source, and keep their reading position while new tokens and tool cards arrive.

### Developer working in a narrow terminal

The developer gets the same meaning as the desktop user, with layout adapted to cell width and
terminal capability. Unsupported media has a useful text fallback rather than a blank region.

### Reviewer reading plans and code review output

The reviewer can rely on tables, nested lists, task state, references, and fenced examples to
retain their original structure. Switching to Source mode does not alter the content.

### Maintainer extending Markdown behavior

The maintainer adds a semantic node once, supplies explicit GUI and TUI behavior, and validates
it against a shared conformance corpus instead of adding another line-oriented special case.

## Goals

P0 must:

1. make CommonMark 0.31.2 plus all five GFM extensions the documented Warposs dialect;
2. preserve exact source and byte ranges for copy, links, streaming invalidation, and debugging;
3. render the same semantic document in GUI and TUI, allowing presentation-specific layouts;
4. remain visually stable and responsive during streaming, resizing, and tool-call interleaving;
5. preserve existing rich code, table, image, Mermaid, and Warp fenced behaviors;
6. provide safe, explicit degradation for unsupported, malformed, oversized, or untrusted input;
7. handle CJK, emoji, combining sequences, and terminal cell widths without corrupt wrapping; and
8. ship with measurable conformance, performance, privacy, and rollback gates.

## Non-goals

P0 does not:

- build a WYSIWYG or block-based Markdown editor;
- make desktop and TUI pixel-identical;
- execute raw HTML, JavaScript, Markdown links, diagrams, or fenced commands;
- automatically fetch a remote image or other URL solely because it appears in agent output;
- add editable task lists, math typesetting, footnotes, an outline, or heading-anchor navigation;
- replace the existing code editor, syntax highlighter, image viewer, table view, or Mermaid
  implementation;
- define a plugin API for arbitrary Markdown dialects;
- reinterpret `warp-runnable-command` as user authorization to run a command; or
- claim notebook editing, Drive export, or every legacy Markdown consumer has migrated until its
  compatibility suite passes.

## Product principles

1. **Source is the authority.** Rendered output is a projection. Copy Source always returns the
   original bytes after transport-level decoding, not reconstructed Markdown.
2. **One message ID, one document.** A tool card may interrupt presentation but cannot reset the
   Markdown state of chunks carrying the same message ID.
3. **Semantic parity, adaptive presentation.** GUI and TUI must preserve the same hierarchy and
   actions; each may choose a layout appropriate to pixels or cells.
4. **Never fail blank.** Unknown, malformed, oversized, or unsupported content remains visible
   as literal source or an explicit fallback with access to Source mode.
5. **Interaction is typed.** Web URLs, local file references, Warp actions, images, and runnable
   commands are distinct targets with distinct trust rules.
6. **Rich rendering is bounded.** Expensive highlighting, diagrams, images, tables, and layout
   have byte, node, time, and dimension limits.
7. **Theme roles, not hard-coded colors.** Meaning survives light/dark themes, reduced color,
   and no-color terminals.

## Obsidian reference profile for the desktop reader

Warposs uses Obsidian's documented reading model as a behavioral and visual reference, not as a
runtime dependency. The relevant public contracts are:

- **Reading view** presents a clean document without Markdown syntax; **Source mode** presents the
  exact syntax. Warposs uses the same user-facing names for source-backed file Markdown.
- **Readable line length** limits the maximum prose width. Warposs centers a bounded reading
  column while allowing intrinsically wide content, especially tables and code, to remain
  horizontally accessible.
- Text and monospace fonts, font size, accent, surfaces, borders, muted text, headings, code,
  tables, and blockquotes are semantic theme roles. Warposs maps those roles to existing Warp
  theme and font settings instead of importing CSS values.
- Reading text remains pointer-selectable and copyable. Switching view must not mutate the
  document, selection source, terminal buffer, or active Agent process.

The initial reproducible Warposs profile uses a 700 px maximum reading column, 1.6 body line
height, 1.5 code line height, a six-level heading scale, subtle one-pixel rules, surfaced code,
and bordered/striped tables. These are Warposs-owned tuning values derived from visual comparison;
they are not asserted to be private Obsidian defaults.

"1:1" in this project means the named interaction states, information hierarchy, spacing rhythm,
and theme-role behavior are compared against fixed visual fixtures at the same viewport and font.
It does not mean copying Obsidian's packaged CSS/assets or promising cross-platform pixel identity.
Live Preview, WYSIWYG editing, heading folding, and proprietary plugin/theme behavior remain out of
scope for P0.

### Reader provenance and capability matrix

| Input | Shared bottom layer | Reading input | Source mode / Copy Source |
| --- | --- | --- | --- |
| Markdown file | `MarkdownReaderDocument` + `MarkdownReaderView` + scoped rich-text style profile | A projection of the exact file source | Available; exact bytes, including line endings, remain authoritative |
| External Agent CLI tab | Same document/view/style types | A bounded visual snapshot of the retained active-block terminal grid | Unavailable; right-click returns to the live terminal and selected rendered text can be copied |

Until the V2 IR lands, blockquotes use a fenced-code-aware visual rail projection in the shared
reader because the legacy `FormattedTextLine` model has no structural blockquote node. The exact
file source remains separate and copyable; the CLI path remains explicitly labeled a visual
snapshot. This compatibility projection must be removed when structural blockquotes migrate to
the source-preserving IR.

## Scope and surfaces

| Surface | P0 contract | P0 exit condition |
| --- | --- | --- |
| Desktop Agent transcript | Full dialect, streaming, source toggle, specialized blocks, links | GUI integration test plus real-display walkthrough |
| Desktop plan/review/read-only Agent Markdown | Same parser and semantic nodes; existing surface-specific controls remain | Shared corpus plus focused view tests |
| Headless TUI Agent transcript and plans | Same semantics, cell-aware layout, safe fallbacks, terminal links where supported | Render-to-lines tests plus real terminal walkthrough |
| Notebook/file Markdown consumers | Must not regress; parser adapter must preserve existing extensions and round trips | Compatibility suite before old parser removal |
| Editable Markdown and Mermaid | Existing behavior remains authoritative | Existing focused tests pass; no editor redesign in P0 |
| Export/clipboard | Exact source remains available; rendered text export remains intentionally separate | Round-trip and copy-target tests |
| External CLI inline tab | Right-click visual snapshot of the complete retained active-block grid; original PTY remains live and recoverable | Source-eligibility tests, context-menu/view tests, and a real Codex CLI walkthrough |

An implementation may enable one surface earlier during dogfood, but release promotion requires
all P0 surfaces in this table.

## User-visible flow

1. Eligible content opens in **Reading** mode. Streaming content continues to update in the same
   logical document even when tool cards appear between text chunks.
2. The focused message/document action menu exposes **View Source**, **View Reading**, **Copy
   Source**, and, where applicable, **Copy rendered text**. Code blocks retain their separate
   **Copy Code** action.
3. The Command Palette exposes **Toggle Markdown Source for Focused Message**. The TUI exposes the
   equivalent through its keyboard-reachable message actions. P0 adds no global default shortcut,
   avoiding an unreviewed collision with existing bindings.
4. Source mode uses the existing monospace/code presentation, labels itself as source, and follows
   the same live source while a response streams. Switching back renders that exact current source.
5. Source/rendered choice is per document and view lifetime. P0 does not persist it into the
   transcript, sync it, or add a global “always source” preference.
6. A malformed, unsupported, unsafe, or oversized rich block shows a concise localized notice and
   a literal fallback. It never replaces the whole document with an empty error view.
7. Activating a link routes through its typed action. A blocked target remains selectable/copyable
   as text and explains that it was not opened without exposing hidden policy or credential data.
8. In a non-empty inline external Agent CLI tab, right-click exposes **View tab in Markdown
   reader**. The reader captures the active block's complete retained output, including scrollback,
   and labels itself **visual snapshot**. Rendered text supports pointer selection and explicit
   clipboard copy. Its menu exposes **Copy** when text is selected and **Show terminal output**; a
   new agent state returns to the live PTY. The action is intentionally absent while the CLI owns
   the alternate screen because Warposs retains only that mode's current viewport. The reader never
   implies that terminal text is the original Markdown source.

## Dialect and feature contract

The standards baseline is [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/) with the
[GitHub Flavored Markdown specification](https://github.github.com/gfm/). “GFM” in this PRD
means tables, task list items, strikethrough, autolinks, and tag filtering; footnotes and alerts
are not part of the formal GFM extension set.

| Construct | P0 | P1 | P2 / deferred |
| --- | --- | --- | --- |
| ATX and Setext headings, H1-H6 | Distinct semantic levels and theme roles | Heading anchors/outline | — |
| Paragraphs, soft/hard breaks, escapes, entities | Full CommonMark semantics | — | — |
| Blockquotes and nested block content | Full semantics, nesting retained | GitHub alert presentation | Custom admonition dialects |
| Ordered, unordered, mixed, loose/tight lists | Full semantics and source start number | — | Editable tasks |
| Fenced and indented code | Rich specialized view with literal fallback | Fold/expand policy refinements | Executable custom extensions |
| Emphasis, strong, inline code, strikethrough | Full supported semantics | Highlight extension if justified | Arbitrary inline plugins |
| Inline, reference, and autolinks | Full semantics and typed targets | Heading/file navigation refinements | Wiki links |
| GFM tables | Alignment, inline formatting, responsive fallback | Full-screen/horizontal table exploration | Editable tables |
| GFM task lists | Read-only checked state | — | User-editable state |
| Images | Alt text and typed source always; existing trusted GUI image path retained | Capability-gated terminal graphics | Automatic remote fetch |
| Raw HTML | Never executed; safe literal/fallback behavior | Allowlisted semantic subset only if justified | General HTML rendering |
| Mermaid fences | Preserve existing GUI behavior; source fallback in unsupported surfaces | TUI preview only with a safe capability design | Other diagram engines |
| Footnotes | Literal, recoverable source | Semantic references and backlinks | — |
| Math | Literal, recoverable source | Evidence-gathering prototype only | Native typesetting after separate approval |
| Warp fenced extensions | Preserve current parsing and typed specialized behavior | Versioned extension metadata | Third-party extensions |

## Functional requirements

### Compatibility and source fidelity

- **MD-COMP-01:** The shared parser MUST represent every CommonMark 0.31.2 block and inline
  construct needed by the P0 matrix without flattening structural nesting.
- **MD-COMP-02:** The parser adapter MUST implement all five GFM extensions as one named dialect
  profile; individual call sites MUST NOT silently select a smaller dialect.
- **MD-COMP-03:** Every semantic node MUST carry one or more byte ranges in the authoritative
  source. A reference-style link, for example, retains both its use-site and definition provenance.
  Any normalized display text MUST remain separate from those source ranges.
- **MD-COMP-04:** `warp-embedded-object`, `warp-runnable-command`, and `warp-markdown-table`
  fences MUST preserve current source compatibility and remain typed nodes after migration.
- **MD-COMP-05:** Existing Mermaid, table, image, and code sections MUST be derived from the same
  document identity as surrounding prose. Pre-segmentation MUST NOT change Markdown meaning.
- **MD-COMP-06:** Finalized streaming output MUST produce the same semantic tree and visible text
  as a one-shot parse of the same source at the same width and theme.
- **MD-COMP-07:** Parse failure MUST preserve the exact source, emit a non-content diagnostic,
  and render a literal fallback; it MUST NOT drop the message.

### Reading and recovery

- **MD-UX-01:** Each eligible source-backed document or agent message MUST offer Reading and Source
  modes. A terminal-derived visual snapshot MUST instead offer Reading and return-to-terminal
  actions because it has no authoritative Markdown source.
  The toggle MUST be keyboard accessible and MUST preserve scroll position as closely as the
  source-to-node map permits.
- **MD-UX-02:** Copy Source MUST copy exact authoritative source. Copy rendered text and Copy Code
  MUST be separately named actions and MUST never substitute for Copy Source.
- **MD-UX-03:** H1-H6 MUST remain distinguishable without relying only on color. TUI styles may
  reuse typography when terminal capabilities are limited but MUST retain a visible level cue.
- **MD-UX-04:** Blockquotes MUST retain nesting and a visible quote boundary. Nested code, lists,
  and paragraphs inside the quote MUST keep their structure.
- **MD-UX-05:** Ordered lists MUST retain their source start number. Wrapped list content MUST
  align to content rather than the marker, including CJK and double-width markers.
- **MD-UX-06:** Malformed and incomplete constructs MUST remain readable during streaming. An
  unclosed fence is displayed as pending code only while its document is pending; final malformed
  content remains accessible in Source mode.

### Code blocks

- **MD-CODE-01:** Fenced code MUST show its language token when present, use the existing syntax
  highlighter, and provide Copy Code. Unknown languages fall back to plain monospace text.
- **MD-CODE-02:** Fence metadata used for local file references MUST be parsed as data and
  validated; it MUST NOT be interpolated into shell commands or treated as proof the file exists.
- **MD-CODE-03:** The TUI MUST retain the current 256 KiB / 5,000-line highlighting ceiling or a
  stricter documented bound. Other presenters MUST define equivalent bounded behavior.
- **MD-CODE-04:** When a limit is reached, Warposs MUST preserve the source, render a visible
  truncation/fallback notice, and keep Copy Source available.

### Tables

- **MD-TABLE-01:** GFM alignment and inline content MUST survive parsing. Cell content MUST NOT be
  split with a plain `|` operation after semantic parsing.
- **MD-TABLE-02:** At readable widths, headers, alignment, and column relationships MUST be
  visually clear. Wrapping MUST use display-cell width rather than UTF-8 byte or scalar count.
- **MD-TABLE-03:** When a table cannot remain readable, the presenter MUST use an explicit
  adaptive form: horizontal scrolling on desktop or header/value records in the TUI. It MUST NOT
  silently discard extra cells or columns.
- **MD-TABLE-04:** Streaming MUST hold back an ambiguous header/separator tail until it is known to
  be a table or plain text. Earlier stable blocks MUST not flicker.
- **MD-TABLE-05:** Empty, ragged, escaped-pipe, inline-code, link, and wide-Unicode cells MUST be in
  the shared corpus.

### Links, images, and embedded content

- **MD-LINK-01:** Visible link text and link destination MUST be separate semantic values. Wrapped
  lines MUST retain the destination over the correct visible cell range.
- **MD-LINK-02:** `http`, `https`, `mailto`, local file references, and typed Warp actions MUST be
  classified before presentation. Unknown or dangerous schemes render as text and are not opened.
- **MD-LINK-03:** Desktop links MUST use the existing typed click path. TUI links SHOULD expose
  semantic/OSC 8 hyperlink metadata when the terminal supports it and MUST retain a readable
  text fallback when it does not.
- **MD-LINK-04:** Local file targets MUST resolve relative to the message/session working
  directory captured with that document, never the renderer process's incidental current
  directory. The UI MUST display the actual normalized target, not a misleading label alone.
- **MD-LINK-05:** Link destinations longer than 8 KiB MUST remain copyable as source but MUST NOT
  become interactive.
- **MD-MEDIA-01:** An image MUST always expose alt text and source. A failed, blocked, unsupported,
  or disabled image renders that fallback rather than blank space.
- **MD-MEDIA-02:** V2 MUST NOT introduce automatic remote fetching. Any existing image fetch path
  must continue through its established trust, size, and cancellation policy.
- **MD-MEDIA-03:** Mermaid and Warp embedded nodes MUST have a source fallback and a bounded error
  state. Rendering failure MUST not invalidate adjacent Markdown.

### Streaming and layout stability

- **MD-STREAM-01:** The agent message ID is the Markdown document identity. Tool calls, status
  cards, reasoning cards, and other interleaved presentation entries MUST NOT terminate parsing
  for later chunks with that same message ID.
- **MD-STREAM-02:** Completed top-level blocks form an append-only stable prefix. Only the mutable
  trailing block set may be replaced during an ordinary append.
- **MD-STREAM-03:** Reference definitions, tables, and other constructs that can retroactively
  change interpretation MUST use explicit invalidation/holdback rules. The implementation MUST
  favor correctness over prematurely declaring a prefix stable.
- **MD-STREAM-04:** The collector MUST handle arbitrary UTF-8-safe chunk boundaries, split fence
  markers, split reference definitions, CRLF, and a final chunk without a newline.
- **MD-STREAM-05:** Width, theme, source, trust context, and working directory are render-cache
  inputs. A cache entry MUST NOT cross any of those boundaries.
- **MD-STREAM-06:** Appending or resizing MUST preserve selection and follow-tail behavior. Warposs
  MUST NOT force a user who scrolled up back to the bottom.

### Accessibility and international text

- **MD-A11Y-01:** All visual roles MUST use theme tokens and remain meaningful in light, dark,
  high-contrast, 16-color, and no-color modes.
- **MD-A11Y-02:** No state—including task completion, link identity, truncation, warning, or
  heading level—may be communicated by color alone.
- **MD-A11Y-03:** Keyboard users MUST be able to toggle Source mode, focus/open links, and invoke
  copy actions without a mouse on surfaces that already expose focus navigation.
- **MD-I18N-01:** Wrapping and truncation MUST be tested with CJK, emoji ZWJ sequences, combining
  marks, variation selectors, and ambiguous-width characters. No operation may split a grapheme.
- **MD-I18N-02:** Fallback labels and errors MUST use the existing localization system. Source,
  code, URL, and file path bytes MUST not be translated.

### External CLI visual projection

- **MD-CLI-01:** A Markdown-reader action MUST appear only for a tracked Agent CLI session with
  non-empty inline active-block output. It MUST remain unavailable in alternate-screen mode because
  that buffer has no retained scrollback. It MAY be available while an Agent CLI is still running
  because the input is an atomic visual snapshot rather than a finalized response.
- **MD-CLI-02:** The CLI PTY MUST remain live and unmodified while its Markdown projection is
  visible. Switching modes MUST NOT restart the CLI, rewrite retained output, or inject bytes into
  the process.
- **MD-CLI-03:** The right-click menu MUST use explicit directional labels: **View tab in Markdown
  reader** and **Show terminal output**. P0 does not promise a keyboard shortcut for this bounded
  visual-reader experiment.
- **MD-CLI-04:** The reader MUST label the projection as a visual snapshot and MUST NOT offer Copy
  Source or claim exact Markdown recovery. Fences, language IDs, table alignment intent, hidden
  link destinations, HTML, and distinctions represented only by terminal styling may already be
  lost. It MUST NOT open a `transcript_path` supplied by a terminal event.
- **MD-CLI-05:** Extraction MUST cover all retained active-block rows, including flat-storage
  scrollback, join terminal soft wraps without inserting hard breaks, and respect the same visual
  secret obfuscation policy as the live terminal. Creating the projection MUST NOT mutate the live
  terminal selection or the system clipboard.
- **MD-CLI-06:** A visual snapshot larger than 2 MiB MUST remain on the PTY path. Parsing or layout
  failure leaves the original terminal output usable.
- **MD-CLI-07:** External CLI Markdown uses the same typed-link, raw-HTML, resource, theme, and
  localization rules as other V2 presenters. Visual extraction does not grant additional trust.
- **MD-CLI-08:** Rendered visual-snapshot text MUST support pointer selection. The standard Copy
  action and the selection context menu MUST copy only the selected rendered text after explicit
  user input. They MUST NOT expose or label that text as original Markdown source.

## Product-level architecture

The required ownership boundary is:

```text
authoritative source + document context
                 |
                 v
     standards parser + Warp extension adapter
                 |
                 v
 semantic document IR + source byte ranges
        /                |                 \
       v                 v                  v
GUI presenter      TUI presenter     source/copy/export
       |                 |
existing rich       cell-aware layout,
child views          capability fallback
```

### Parser decision

P0 SHOULD use
[`pulldown-cmark`](https://github.com/pulldown-cmark/pulldown-cmark/tree/07bae2459d90175b661d42b8acf207382e111ae5)
behind a Warposs-owned adapter. Its pull events and `into_offset_iter()` source ranges match the
streaming and exact-copy requirements, and OpenAI Codex provides a current Rust TUI example of
that architecture.

The P0 profile explicitly enables tables, task lists, and strikethrough, then supplies and tests
the formal GFM extended-autolink and tag-filter behavior in the adapter. At the inspected commit,
`pulldown-cmark`'s `ENABLE_GFM` option concerns GitHub alert blockquotes; it is not shorthand for
all five formal GFM extensions and remains off until the P1 alert decision.

The dependency is admitted only after a spike proves:

1. all applicable CommonMark/GFM corpus cases map into the proposed IR;
2. Warp fenced extensions preserve their exact source and typed behavior;
3. final output is equivalent across adversarial chunk boundaries; and
4. performance budgets in this PRD pass on the benchmark runner.

If the spike fails, the team must return to parser design review. It must not silently fork
`pulldown-cmark` or switch the product dialect. Comrak remains the evaluated AST-oriented
alternative, not a second parser shipped beside it.

### Semantic IR requirements

The IR MUST:

- distinguish block containers from inline content and preserve arbitrary valid nesting;
- store source byte ranges and stable node identities derived from document identity plus range;
- represent typed link targets, code metadata, table alignment, task state, image data, and Warp
  extensions without embedding GUI or TUI types;
- distinguish a finalized node from a mutable streaming tail;
- allow a presenter to request literal source for any node; and
- remain serializable only for tests/debugging unless a separate persistence contract is
  approved. It is not a new transcript storage format in P0.

### Migration boundary

The current `FormattedText` model remains behind a compatibility adapter while consumers migrate.
The old and new parsers run in parallel only in shadow/dogfood comparison. The old parser cannot
be deleted until notebook, editor, Agent, TUI, export, and clipboard compatibility suites pass.

Dual parsing MUST NOT log source, rendered text, URL values, file paths, or code. A semantic diff
may record only node kinds, counts, error class, timing, byte-size buckets, and a random local
correlation ID.

## Safety and privacy

- **MD-SEC-01:** Markdown source, HTML, URLs, titles, alt text, fence metadata, and embedded YAML
  are untrusted input.
- **MD-SEC-02:** Raw HTML is never executed. Scriptable or disallowed HTML is escaped or shown as
  literal source under the tag-filter contract.
- **MD-SEC-03:** C0/C1 control characters and ANSI/OSC sequences from Markdown MUST NOT reach a
  terminal control channel. Newline and tab are normalized only for presentation; source remains
  available unchanged.
- **MD-SEC-04:** Opening a target uses the typed URI/file action layer and its existing user
  confirmation and policy checks. Markdown does not bypass them.
- **MD-SEC-05:** Parsing and rendering enforce limits for source bytes, nesting depth, node count,
  URL length, table dimensions, image bytes/pixels, diagram time, and syntax-highlight bytes.
- **MD-SEC-06:** A document beyond the rich-render limit switches to a bounded literal or
  virtualized mode with a visible notice. It is not discarded or partially executed.
- **MD-SEC-07:** Telemetry and logs MUST NOT contain Markdown source, rendered content, code,
  URLs, local paths, image data, embedded payloads, clipboard contents, or secrets.
- **MD-SEC-08:** Fuzzing MUST cover parser panics, pathological nesting, quadratic input,
  oversized link/image metadata, control injection, and malformed UTF-8 at the transport boundary.

P0 rich parsing is bounded to 1 MiB per logical document and 100,000 semantic nodes. Reaching
either limit triggers the literal/virtualized fallback. Individual specialized presenters may
apply stricter existing limits. These bounds do not truncate the authoritative source retained by
the owning message model.

## Performance and reliability budgets

The implementation must add a checked-in `warp-markdown-bench` corpus with prose, nested lists,
code, wide tables, links, CJK/emoji, and adversarial delimiter input. Measurements use release
builds on the designated Linux x86_64 CI performance runner, 10 warm-up iterations, and at least
50 measured iterations; the benchmark records runner identity and baseline commit.

| Scenario | P0 budget |
| --- | --- |
| Parse plus IR construction, finalized 100 KiB mixed document | p95 <= 25 ms |
| First visible presentation, 100 KiB mixed document at 120 columns | p95 <= 100 ms |
| Append 1 KiB to a 100 KiB stream without retroactive invalidation | p95 <= 16 ms |
| Append requiring table/reference tail invalidation | p95 <= 50 ms |
| Resize a transcript with 100 visible Markdown blocks | p95 <= 100 ms, without losing scroll/selection |
| Source growth from N to 2N over fixed-size chunks | total parser work <= 2.5x, guarding against quadratic growth |
| Oversized document fallback | notice and usable Source mode within 100 ms after the limit is detected |

A single expensive rich child may finish asynchronously, but the UI thread must yield within one
16.7 ms frame slice. Cancellation is required when source, width, theme, or owning view changes.

Reliability gates:

- zero panics or blank documents over the conformance, regression, and fuzz corpora;
- 100% finalized-stream versus one-shot semantic equivalence over the chunk permutation suite;
- 100% exact Copy Source round trips for accepted transport input; and
- zero known high-severity control-sequence, unsafe-link, or remote-fetch regressions.

## Observability and success measures

All measurements are subject to the user's existing telemetry consent.

Allowed content-free events and fields:

| Event | Allowed fields |
| --- | --- |
| `markdown_render_completed` | surface, mode, source-size bucket, node-count bucket, latency bucket, fallback reason, feature-flag cohort |
| `markdown_stream_updated` | surface, appended-size bucket, stable-prefix bucket, invalidation class, latency bucket |
| `markdown_mode_changed` | surface, from/to rendered/source, document-size bucket |
| `markdown_link_action` | target class only (`web`, `mail`, `local_file`, `warp_action`, `blocked`), outcome class |
| `markdown_render_failed` | parser/presenter error class, surface, size bucket, fallback shown boolean |

P0 release targets after a 14-day preview window are:

1. parser/presenter fallback rate below 0.1% of eligible documents, excluding the documented
   1 MiB limit;
2. blank-render rate of zero;
3. at least 99% of ordinary streaming appends within the append latency budget;
4. no statistically significant regression in transcript scroll or input latency versus control;
5. at least 50% fewer Markdown-rendering bug reports per eligible active user than the preceding
   30-day baseline; and
6. no privacy review finding that content-bearing fields can enter telemetry or logs.

Source-mode usage is diagnostic, not automatically a failure: users may prefer source. A rise
paired with a parser fallback or bug report is investigated; it is not used to hide the feature.

## Rollout and rollback

The feature uses one product-level `WarpossMarkdownRenderingV2` flag. Lower-level experiments may
exist in tests, but users must not need to coordinate multiple flags to get a coherent dialect.

### Stage 0 — corpus and shadow parse

- Land the IR, adapter, conformance corpus, security fixtures, and content-free semantic diff.
- Keep current presentation authoritative.
- Exit only when compatibility, performance, and privacy gates pass in CI.

### Stage 1 — dogfood

- Enable V2 for internal/dogfood builds on desktop and TUI.
- Keep per-document Source mode and a flag rollback.
- Run macOS, Windows, and Linux verification, including narrow terminals and reduced-color modes.
- Exit after seven consecutive days with no unresolved severity-1/2 regression and budgets met.

### Stage 2 — preview

- Promote through the repository's normal preview flag path.
- Compare fallback, latency, raw-toggle correlation, and bug-report rates with control.
- Pause promotion on any semantic data loss, unsafe link, scroll instability, or performance gate
  breach.

### Stage 3 — release and cleanup

- Promote only after the 14-day success window and product/security/desktop/TUI sign-off.
- Retain the old parser rollback for one release cycle.
- Remove the flag and compatibility path in a separate change after stability is proven; use the
  repository's feature-flag removal workflow rather than combining cleanup with rollout.

Rollback switches the authoritative presenter/parser path; it does not delete source, rewrite
transcripts, or change persisted data.

## Validation and acceptance plan

### Automated validation

1. **Standards conformance:** execute every applicable official CommonMark 0.31.2 and GFM
   extension example against semantic expectations. Safe raw-HTML behavior is validated against
   the product security contract rather than browser HTML output.
2. **Source fidelity:** golden source-to-IR ranges, exact Copy Source, CRLF, Unicode, reference
   definitions, escaped delimiters, and Warp fenced extension round trips.
3. **Streaming equivalence:** split each corpus document at every delimiter boundary and a sampled
   set of every UTF-8 boundary; interleave synthetic tool cards without changing message ID; assert
   the finalized tree and visible text equal one-shot rendering.
4. **Presenter parity:** shared semantic snapshots plus GUI view tests and TUI render-to-lines
   tests. Layout may differ; headings, block ownership, text, links, task state, fallbacks, and copy
   targets may not.
5. **Responsive layout:** widths 20, 40, 80, 120, and 240 cells/columns; CJK/emoji and ragged/wide
   tables; theme and no-color matrices.
6. **Security:** fuzz/property tests, scheme filtering, ANSI/OSC injection, HTML/script payloads,
   oversized metadata, deep nesting, cancellation, and remote-resource assertions.
7. **Performance:** checked-in benchmarks enforce the budgets above and archive baseline/results.
8. **Legacy compatibility:** existing markdown parser, Agent segmentation, notebook, Mermaid,
   table, image, export, and clipboard tests pass before old-parser removal.
9. **External CLI visual bridge:** inline active-block scrollback, terminal soft wraps, explicit
   alternate-screen rejection, bullet/rule normalization, GFM structures that remain visible,
   empty/oversized snapshots, new-turn, secret obfuscation, Reader selection/copy, and right-click
   round trips are covered without reading transcript paths, mutating the live terminal selection,
   writing the clipboard without explicit user input, or mutating the PTY.

### Runtime acceptance

Automated checks are necessary but do not prove the result on screen.

- Desktop: run the real GUI on macOS, Windows, and Linux; verify a streaming fixture, tool-card
  interleaving, selection, scroll retention, Source toggle, link opening, code copy, wide table,
  blocked image, Mermaid success/failure, light/dark/high-contrast themes, and 200% scale. Use the
  checked-in [`obsidian-reader-reference.md`](fixtures/obsidian-reader-reference.md) for the shared
  desktop reading-width, typography, code, table, quote, and selection checks.
- TUI: run `./script/run-tui` in a real terminal; verify the same semantic fixture at narrow and
  wide widths, 16-color/no-color, OSC 8 supported/unsupported, keyboard actions, and resize.
- Cross-front-end: save the exact source and semantic snapshot from both paths and compare them.

### P0 release acceptance checklist

- [ ] `MD-COMP-*`, `MD-UX-*`, `MD-CODE-*`, `MD-TABLE-*`, `MD-LINK-*`, `MD-MEDIA-*`,
      `MD-STREAM-*`, `MD-A11Y-*`, `MD-I18N-*`, `MD-CLI-*`, and `MD-SEC-*` requirements have tests
      or recorded runtime evidence.
- [ ] Applicable CommonMark/GFM cases pass with no undocumented exclusions.
- [ ] Finalized streaming equals one-shot parsing across the chunk/interleaving suite.
- [ ] Exact source remains available for every fallback and oversized case.
- [ ] GUI and TUI runtime acceptance is recorded for all three supported desktop OS families.
- [ ] Performance, privacy, and security gates pass on the release candidate.
- [ ] Existing Mermaid/table/image/notebook/editor compatibility suites pass.
- [ ] Rollback is exercised without transcript or source loss.
- [ ] Product, engineering, security, accessibility, desktop QA, and TUI QA sign off.

## Delivery slices

### M0 — contract fixtures and benchmarks

Deliver the corpus, semantic expectation format, streaming interleaving fixtures, security
fixtures, benchmark harness, and baseline measurements. No user-visible behavior changes.

Exit: test data and measurement methodology are reviewable and deterministic.

### M1 — parser adapter and semantic IR

Deliver `pulldown-cmark` adapter, source ranges, typed targets, Warp extension adapter, literal
fallback, dual-parse metrics, and compatibility bridge to `FormattedText`.

Exit: standards, source-fidelity, stream-equivalence, security, and parser performance gates pass.

### M2 — desktop presenter

Connect the IR to existing desktop rich child views, add blockquote/heading/list semantics, source
mode, typed links, stable streaming prefix, and responsive table behavior.

Exit: desktop automated and real-display acceptance passes behind the flag.

### M3 — TUI presenter

Add distinct heading cues, blockquotes, semantic hyperlinks, source mode, persistent rich code
hooks, stable streaming, and capability-aware fallbacks to the existing TUI renderer.

Exit: TUI render-to-lines, real-terminal, width, Unicode, and capability acceptance passes.

### M4 — hardening, rollout, and cleanup

Complete cross-platform verification, dogfood/preview observation, rollback exercise, release
promotion, one-cycle fallback retention, and later flag/parser cleanup.

Exit: all P0 release checklist items are signed off; cleanup is a separately reviewable change.

## Risks and mitigations

| Risk | Impact | Mitigation / gate |
| --- | --- | --- |
| Parser migration changes notebook or export behavior | Source corruption or compatibility regression | Compatibility adapter, exact-source tests, no old-parser removal before consumer matrix passes |
| Specialized Agent pre-segmentation creates two document interpretations | Broken nesting or stream state | One semantic document identity; specialized nodes derived from IR/source ranges |
| Tool cards split an open fence/table/list | Prose and code swap roles mid-answer | Message-ID invariant and explicit interleaving fixtures inspired by Zed issue #62631 |
| Reference definitions alter earlier links | Incorrect “stable” prefix | Source-offset invalidation and conservative mutable tail |
| Tables become unreadable or quadratic | UI freeze or lost cells | Width-aware algorithm, record fallback/scroll, dimensions and benchmark limits |
| OSC/HTML/link content escapes the renderer | Terminal control or unsafe action | Typed targets, control stripping, safe raw HTML, scheme and length gates, fuzzing |
| A terminal event points at an arbitrary transcript file | Local data disclosure | Never follow event-supplied transcript paths; accept bounded response content over the protocol or a separately authenticated internal handle |
| Codex's 200-character event response is treated as lossless source | Misleading partial answer | Keep it out of the visual-reader path and add protocol completeness/truncation metadata before exact-source acceptance |
| A visual snapshot is mistaken for original Markdown | Incorrect source or semantic claims | Label it visual snapshot, omit Copy Source, preserve the live PTY, and document irrecoverable syntax/style distinctions |
| Rich images/diagrams cause network or resource abuse | Privacy, memory, or availability incident | No new automatic fetch, bounded/cancellable presenters, source fallback |
| Unicode width differs by platform/terminal | Misalignment and broken links | Grapheme/cell-width abstraction and cross-platform capability matrix |
| Dual parse leaks content through diffs | Privacy regression | Content-free schema and privacy review before dogfood |
| Large shared IR expands scope indefinitely | Delayed delivery | P0 feature matrix fixed; footnotes, alerts, math, editing, and plugins stay phased |

## GitHub project research

The comparison prioritizes source architecture and failure modes over popularity metrics. Links
below pin the inspected commit where practical.

| Project | Verified observation | What Warposs should reuse | Adoption decision | License |
| --- | --- | --- | --- | --- |
| [Obsidian](https://obsidian.md/help/edit-and-read) | Official Help separates Reading view from Editing view and Source mode, documents a readable-line-length setting, and exposes text/monospace fonts and note font size. Official developer guidance recommends semantic CSS variables for theme compatibility; the public [sample theme](https://github.com/obsidianmd/obsidian-sample-theme) is a minimal theme-development template. | Reading/Source vocabulary, bounded readable width, semantic visual roles, selectable reading content, and visual-regression fixtures. | Behavior and visual reference only. Do not embed Obsidian, scrape packaged application styles, or copy proprietary assets. The sample theme is 0BSD; no sample-theme code is required by this implementation. | Proprietary app; public sample theme 0BSD |
| [OpenAI Codex](https://github.com/openai/codex/tree/694edc23b22b4696400dc47663ecacd437623870) | Rust TUI uses `pulldown-cmark` events/source offsets, H1-H6 roles, blockquotes, nested lists, syntax highlighting, width-aware tables with record fallback, typed terminal hyperlinks, and a streaming stable-prefix/tail controller. [Renderer](https://github.com/openai/codex/blob/694edc23b22b4696400dc47663ecacd437623870/codex-rs/tui/src/markdown_render.rs), [stream controller](https://github.com/openai/codex/blob/694edc23b22b4696400dc47663ecacd437623870/codex-rs/tui/src/streaming/controller.rs), [collector](https://github.com/openai/codex/blob/694edc23b22b4696400dc47663ecacd437623870/codex-rs/tui/src/markdown_stream.rs). | Source-backed IR, stable prefix/mutable tail, conservative table/reference invalidation, hyperlink metadata, semantic table fallback. | Reference architecture only; WarpUI and transcript models differ. | Apache-2.0 |
| [Glamour](https://github.com/charmbracelet/glamour/tree/d0a719943b7b399fc17f0a98454c7b70443ce29b) | Go renderer exposes stylesheet roles and width configuration; v2 adds grapheme/cell-aware wrapping, OSC 8 links, and improved tables. | Semantic style roles, capability separation, Unicode wrapping test cases. | Do not adopt: Go/ANSI full-string output cannot supply WarpUI entities, source spans, or streaming child views. | MIT |
| [Gemini CLI](https://github.com/google-gemini/gemini-cli/tree/3c311beac2e78336816dd4a123db39743f9fbf85) | Ink UI offers a syntax-highlighted raw Markdown mode and bounds pending code by terminal height, but [`MarkdownDisplay.tsx`](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/packages/cli/src/ui/utils/MarkdownDisplay.tsx) recognizes core blocks with line/regex logic. | Source-mode escape hatch and explicit pending-state bounds. | UX reference only; do not reproduce the line/regex parser architecture. | Apache-2.0 |
| [mdcat maintained fork](https://github.com/BIRSAx2/mdcat/tree/4a9bc02556129489ab65e56be4b84fb3987138a9) | The original `swsnr/mdcat` repository identifies this as its maintained fork. The Rust terminal renderer combines syntax highlighting, OSC 8, terminal capability detection, themed headings/alerts, inline images, and capability-specific math/Mermaid fallbacks; its README also states current table-wrapping limits. | Capability-based degradation, explicit remote/local resource policy, and useful text/Unicode fallback when graphics are unavailable. | Behavior reference only; it is a whole terminal application rather than a WarpUI presenter. Any code reuse requires an MPL-2.0 review. | MPL-2.0 |
| [tui-markdown](https://github.com/joshka/tui-markdown/tree/65514ddefb6526be7d14ab0e734954b8baea04c9) | Native Rust/Ratatui conversion demonstrates table, syntax-highlight, and Markdown-reader composition, but the project describes itself as an experimental proof of concept. | Fixture/API comparison for a native Rust TUI presenter. | Do not embed; Warposs needs its own `TuiElement` tree, streaming identity, actions, and persistent code views. | MIT OR Apache-2.0 |
| [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark/tree/07bae2459d90175b661d42b8acf207382e111ae5) | Fast Rust pull parser targets CommonMark compliance and exposes `(Event, Range)` through `into_offset_iter`; optional tables, tasks, strikethrough, and footnotes exist. Its current `ENABLE_GFM` flag covers alert blockquotes rather than the complete formal five-extension profile. | P0 parser candidate and source-range foundation; the adapter owns complete GFM-profile conformance. | Adopt behind a Warposs adapter after the defined spike gate. | MIT |
| [Comrak](https://github.com/kivikakk/comrak/tree/c5a3b45ce94460e37c54408cd51d9b526e8cb4d8) | Rust CommonMark 0.31.2/GFM parser offers an arena AST, source positions, safe-by-default HTML, and many extensions including alerts, footnotes, and math. | Alternative if editable AST/extension fidelity later outweighs streaming pull-event needs. | Evaluated fallback, not a parallel P0 dependency. | BSD-2-Clause |
| [Zed issue #62631](https://github.com/zed-industries/zed/issues/62631) | A tool card interleaved inside chunks with one message ID split an open code fence into two Markdown documents, while true message boundaries worked. | Concrete regression fixture and the “one message ID, one document” invariant. | Failure-mode evidence, not a dependency. | N/A |
| [Warp Codex plugin](https://github.com/warpdotdev/codex-warp/tree/31ce59d9011cfb1d78f265649a228dac5de58d76/plugins/warp) | The structured Stop hook transports `last_assistant_message` as `response`, but truncates it to 200 characters and separately emits `transcript_path`. | Proves a source-bearing completion channel exists and exposes the protocol gap for lossless full Markdown. | Do not depend on it for the visual-reader experiment; require a versioned payload change for exact-source acceptance. | MIT |

### Research synthesis

The projects converge on five useful patterns:

1. use a standards parser rather than growing regex/line heuristics;
2. preserve source offsets independently of styled output;
3. separate semantic styles from terminal capability and theme;
4. make tables and links width-aware without throwing information away; and
5. treat streaming as an incremental document problem, not repeated rendering of unrelated
   chunks.

They also show what not to combine: a whole-document ANSI renderer cannot own WarpUI actions or
persistent views, a full arena AST does not automatically solve incremental presentation, and
terminal image support must not imply unconditional network access.

## Post-P0 decisions

The following are explicitly decided for this PRD and do not block P0:

- GitHub alerts move to P1 only after blockquote semantics are stable.
- Footnotes move to P1; they are not part of formal GFM.
- Math remains P2 and requires a separate product/security/performance review.
- Automatic remote image loading remains out of scope; terminal graphics require an explicit
  capability and trust design.
- A Markdown outline, hybrid editing, and full-screen table view require separate usage evidence.
- Comrak is reconsidered only if the P0 parser spike fails a named gate or a later editing feature
  requires a mutable AST.

## Evidence references

### Warposs repository

- [`markdown_parser` public model](../../crates/markdown_parser/src/lib.rs)
- [`markdown_parser` implementation](../../crates/markdown_parser/src/markdown_parser.rs)
- [Agent Markdown segmentation](../../app/src/ai/agent/util.rs)
- [Agent text section model](../../app/src/ai/agent/mod.rs)
- [Desktop Agent block renderer](../../app/src/ai/blocklist/block/view_impl/common.rs)
- [TUI Markdown presenter](../../crates/warp_tui/src/tui_markdown.rs)
- [TUI responsive table presenter](../../crates/warp_tui/src/tui_markdown/table.rs)
- [TUI code block view](../../crates/warp_tui/src/tui_code_block_view.rs)
- [CLI-agent event model](../../app/src/terminal/cli_agent_sessions/event/mod.rs)
- [CLI-agent session state](../../app/src/terminal/cli_agent_sessions/mod.rs)
- [External CLI whole-tab Markdown reader experiment](../../app/src/terminal/view/cli_agent_markdown_reader.rs)
- [Obsidian-style desktop reader visual fixture](fixtures/obsidian-reader-reference.md)
- [Mermaid Markdown product contract](../mermaid-markdown-in-plans/PRODUCT.md)
- [Markdown table consistency contract](../zachlloyd/markdown-table-consistency/PRODUCT.md)
- [Wide Markdown table scrolling contract](../zachlloyd/wide-markdown-table-scrolling/PRODUCT.md)

### Standards and upstream source

- [CommonMark 0.31.2](https://spec.commonmark.org/0.31.2/)
- [GitHub Flavored Markdown specification](https://github.github.com/gfm/)
- [Obsidian views and editing modes](https://obsidian.md/help/edit-and-read)
- [Obsidian settings](https://obsidian.md/help/settings)
- [Obsidian styling guidance](https://docs.obsidian.md/Reference/CSS%20variables/About%20styling)
- [Obsidian sample theme](https://github.com/obsidianmd/obsidian-sample-theme)
- [OpenAI Codex Markdown renderer](https://github.com/openai/codex/blob/694edc23b22b4696400dc47663ecacd437623870/codex-rs/tui/src/markdown_render.rs)
- [OpenAI Codex streaming controller](https://github.com/openai/codex/blob/694edc23b22b4696400dc47663ecacd437623870/codex-rs/tui/src/streaming/controller.rs)
- [Glamour](https://github.com/charmbracelet/glamour/tree/d0a719943b7b399fc17f0a98454c7b70443ce29b)
- [Gemini CLI Markdown display](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/packages/cli/src/ui/utils/MarkdownDisplay.tsx)
- [Warp Codex Stop hook](https://github.com/warpdotdev/codex-warp/blob/31ce59d9011cfb1d78f265649a228dac5de58d76/plugins/warp/scripts/on-stop.sh)
- [mdcat maintained fork](https://github.com/BIRSAx2/mdcat/tree/4a9bc02556129489ab65e56be4b84fb3987138a9)
- [Original mdcat maintenance notice](https://github.com/swsnr/mdcat/tree/5012892b6f998545381413d60b30271004b4ad28)
- [tui-markdown](https://github.com/joshka/tui-markdown/tree/65514ddefb6526be7d14ab0e734954b8baea04c9)
- [pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark/tree/07bae2459d90175b661d42b8acf207382e111ae5)
- [Comrak](https://github.com/kivikakk/comrak/tree/c5a3b45ce94460e37c54408cd51d9b526e8cb4d8)
- [Zed streaming Markdown interleaving issue](https://github.com/zed-industries/zed/issues/62631)
