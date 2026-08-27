# Warp localization boundary

`warp_i18n` owns locale selection and user-interface translation catalogs. It
does not depend on WarpUI, application state, terminal rendering, networking,
or account services.

The source fork defaults to Simplified Chinese (`zh-CN`). Set
`WARP_LOCALE=en-US` before launching Warp to use the upstream English strings.
Unknown and untranslated strings fall back to English.

There are three translation paths:

- `localize_static` is the compatibility adapter for borrowed static UI text.
  It deliberately leaves owned strings untouched so terminal output, file
  names, user input, and model responses are not translated by accident.
- `localize_ui` is for an explicit UI boundary such as a button or menu. It may
  translate owned strings because the caller has already classified the value
  as interface text.
- `localize_ref` serves UI APIs that require a borrowed static `&str`, such as
  native dialog titles and CLI prompt help text.

Formatted UI messages use `localize_format!` with named placeholders. The
translated template may reorder those placeholders, while each runtime value
is substituted verbatim and never translated as user or terminal content.
Catalog fields encode literal newlines, tabs, and backslashes as `\n`, `\t`,
and `\\`.

Run `python3 script/i18n/audit_zh_cn.py --check` from the repository root to
validate the catalog. The check is fixed at 100% for the production GUI,
onboarding, editor, WarpUI, and TUI roots. It also rejects unclassified
formatted copy, placeholder drift, stale exclusions, and static formatted-text
spans that have not been routed through an explicit localization boundary. The
extractor resolves statically identifiable UI constants and covers formatted
links, `TextAndIcon` labels, tooltips, placeholders, titles, and TUI span arrays.

Language-neutral layout glyphs and runtime data templates are documented in
`script/i18n/zh_cn_exclusions.json` by source or exact occurrence with a reason;
they are not counted as translated catalog entries. Run
`python3 -m unittest script/i18n/test_audit_zh_cn.py` to validate the extractor.
