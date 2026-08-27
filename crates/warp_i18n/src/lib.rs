//! Locale selection and translation catalogs for Warp's user interface.
//!
//! This crate intentionally has no UI or application dependencies. Callers
//! decide whether a value is user-interface copy before asking for explicit
//! localization; terminal data and user-authored content remain outside this
//! boundary.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

/// Environment variable used to override the locale for a process.
pub const WARP_LOCALE_ENV: &str = "WARP_LOCALE";

const ZH_CN_SOURCE: &str = include_str!("../locales/zh-CN.tsv");

static ACTIVE_LOCALE: OnceLock<Locale> = OnceLock::new();
static ZH_CN_CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Locales currently shipped by this fork.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Locale {
    EnUs,
    /// Default locale for this source fork.
    #[default]
    ZhCn,
}

impl Locale {
    /// Parses common locale spellings without consulting global process state.
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().replace('_', "-").to_ascii_lowercase();
        match normalized.as_str() {
            "en" | "en-us" => Some(Self::EnUs),
            "zh" | "zh-cn" | "zh-hans" | "zh-hans-cn" => Some(Self::ZhCn),
            _ => None,
        }
    }

    /// BCP 47 language tag suitable for text shaping and font fallback.
    pub const fn language_tag(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }
}

/// Returns the process locale, initialized once from [`WARP_LOCALE_ENV`].
pub fn active_locale() -> Locale {
    *ACTIVE_LOCALE.get_or_init(|| {
        std::env::var(WARP_LOCALE_ENV)
            .ok()
            .as_deref()
            .and_then(Locale::parse)
            .unwrap_or_default()
    })
}

/// Localizes borrowed static UI copy while preserving owned values verbatim.
///
/// This is the low-risk compatibility path for the existing `Text` boundary:
/// dynamically constructed terminal, file, user, and model content is usually
/// owned and therefore cannot be translated accidentally.
pub fn localize_static(text: impl Into<Cow<'static, str>>) -> Cow<'static, str> {
    localize_static_for_locale(active_locale(), text.into())
}

/// Localizes a value that the caller has explicitly classified as UI copy.
pub fn localize_ui(text: impl Into<Cow<'static, str>>) -> Cow<'static, str> {
    localize_ui_for_locale(active_locale(), text.into())
}

/// Localizes a static UI string for APIs that require a borrowed static reference.
pub fn localize_ref(source: &'static str) -> &'static str {
    localize_ref_for_locale(active_locale(), source)
}

/// Localizes a named-placeholder template and substitutes its already-rendered values.
///
/// Templates use `{name}` placeholders. Values are treated as opaque user or
/// runtime data and are never passed through the translation catalog.
pub fn format_ui(source: &'static str, arguments: &[(&str, String)]) -> String {
    format_ui_for_locale(active_locale(), source, arguments)
}

/// Formats localized UI copy while keeping dynamic values outside translation.
#[macro_export]
macro_rules! localize_format {
    ($source:literal $(, $name:ident = $value:expr)* $(,)?) => {{
        let arguments = [$(
            (stringify!($name), ($value).to_string()),
        )*];
        $crate::format_ui($source, &arguments)
    }};
}

/// Looks up an exact UI-source translation for the active locale.
pub fn translation(source: &str) -> Option<&'static str> {
    translation_for_locale(active_locale(), source)
}

/// Returns the number of translations shipped for a locale.
pub fn translation_count(locale: Locale) -> usize {
    match locale {
        Locale::EnUs => 0,
        Locale::ZhCn => zh_cn_catalog().len(),
    }
}

#[doc(hidden)]
pub fn localize_static_for_locale(locale: Locale, text: Cow<'static, str>) -> Cow<'static, str> {
    match text {
        Cow::Borrowed(source) => translation_for_locale(locale, source)
            .map(Cow::Borrowed)
            .unwrap_or(Cow::Borrowed(source)),
        Cow::Owned(text) => Cow::Owned(text),
    }
}

#[doc(hidden)]
pub fn localize_ui_for_locale(locale: Locale, text: Cow<'static, str>) -> Cow<'static, str> {
    translation_for_locale(locale, text.as_ref())
        .map(Cow::Borrowed)
        .unwrap_or(text)
}

#[doc(hidden)]
pub fn localize_ref_for_locale(locale: Locale, source: &'static str) -> &'static str {
    translation_for_locale(locale, source).unwrap_or(source)
}

#[doc(hidden)]
pub fn format_ui_for_locale(
    locale: Locale,
    source: &'static str,
    arguments: &[(&str, String)],
) -> String {
    let mut formatted = localize_ui_for_locale(locale, Cow::Borrowed(source)).into_owned();
    for (name, value) in arguments {
        formatted = formatted.replace(&format!("{{{name}}}"), value);
    }
    formatted
}

fn translation_for_locale(locale: Locale, source: &str) -> Option<&'static str> {
    match locale {
        Locale::EnUs => None,
        Locale::ZhCn => zh_cn_catalog().get(source).map(String::as_str),
    }
}

fn zh_cn_catalog() -> &'static HashMap<String, String> {
    ZH_CN_CATALOG.get_or_init(|| {
        parse_catalog(ZH_CN_SOURCE)
            .unwrap_or_else(|error| panic!("invalid embedded zh-CN catalog: {error}"))
    })
}

#[derive(Debug, Eq, PartialEq)]
enum CatalogError {
    MissingSeparator { line: usize },
    ExtraSeparator { line: usize },
    EmptySource { line: usize },
    EmptyTranslation { line: usize },
    InvalidEscape { line: usize, value: String },
    DuplicateSource { line: usize, source: String },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator { line } => {
                write!(formatter, "line {line} is missing a tab separator")
            }
            Self::ExtraSeparator { line } => {
                write!(
                    formatter,
                    "line {line} contains more than one tab separator"
                )
            }
            Self::EmptySource { line } => write!(formatter, "line {line} has an empty source"),
            Self::EmptyTranslation { line } => {
                write!(formatter, "line {line} has an empty translation")
            }
            Self::InvalidEscape { line, value } => {
                write!(formatter, "line {line} contains invalid escape {value:?}")
            }
            Self::DuplicateSource { line, source } => {
                write!(formatter, "line {line} duplicates source {source:?}")
            }
        }
    }
}

fn parse_catalog(source: &str) -> Result<HashMap<String, String>, CatalogError> {
    let mut catalog = HashMap::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() || (line.starts_with('#') && !line.contains('\t')) {
            continue;
        }

        let (encoded_source, encoded_translation) = line
            .split_once('\t')
            .ok_or(CatalogError::MissingSeparator { line: line_number })?;
        if encoded_translation.contains('\t') {
            return Err(CatalogError::ExtraSeparator { line: line_number });
        }
        let source = decode_catalog_field(encoded_source, line_number)?;
        let translation = decode_catalog_field(encoded_translation, line_number)?;
        if source.is_empty() {
            return Err(CatalogError::EmptySource { line: line_number });
        }
        if translation.is_empty() {
            return Err(CatalogError::EmptyTranslation { line: line_number });
        }
        if catalog.insert(source.clone(), translation).is_some() {
            return Err(CatalogError::DuplicateSource {
                line: line_number,
                source,
            });
        }
    }
    Ok(catalog)
}

fn decode_catalog_field(value: &str, line: usize) -> Result<String, CatalogError> {
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let escaped = characters
            .next()
            .ok_or_else(|| CatalogError::InvalidEscape {
                line,
                value: "\\".to_owned(),
            })?;
        match escaped {
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            '\\' => decoded.push('\\'),
            _ => {
                return Err(CatalogError::InvalidEscape {
                    line,
                    value: format!("\\{escaped}"),
                });
            }
        }
    }
    Ok(decoded)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
