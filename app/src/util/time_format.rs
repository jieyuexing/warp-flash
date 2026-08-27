use std::ops::Sub;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Local, Utc};
use warp_i18n::Locale;

// Some conversion ratios for time units.
const SEC_TO_MS: f64 = 1000.;
const MIN_TO_MS: f64 = 60. * SEC_TO_MS;
const HOUR_TO_MS: f64 = 60. * MIN_TO_MS;
const DAY_TO_MS: f64 = 24. * HOUR_TO_MS;
const WEEK_TO_MS: f64 = 7. * DAY_TO_MS;
const MONTH_TO_MS: f64 = 30.44 * DAY_TO_MS;
const YEAR_TO_MS: f64 = 365.25 * DAY_TO_MS;

/// Subtract a given DateTime from now and format the duration is a concise, approximated,
/// human-readable form. e.g. "just now"
pub fn format_approx_duration_from_now(datetime: DateTime<Local>) -> String {
    human_readable_approx_duration(Local::now().sub(datetime), false)
}

/// Subtract a given DateTime from now and format the duration is a concise, approximated,
/// human-readable form. e.g. "Just now"
pub fn format_approx_duration_from_now_sentence_case(datetime: DateTime<Local>) -> String {
    human_readable_approx_duration(Local::now().sub(datetime), true)
}

/// Takes a time in UTC and determines roughly how long ago it occurred.
pub fn format_approx_duration_from_now_utc(datetime: DateTime<Utc>) -> String {
    human_readable_approx_duration(Utc::now().sub(datetime), false)
}

/// Format a duration into a human-readable string, e.g. "3.14 sec".
/// Compared to [`human_readable_approx_duration`], this method is for higher-precision, smaller
/// values.
pub fn human_readable_precise_duration(duration: Duration) -> String {
    human_readable_precise_duration_for_locale(duration, warp_i18n::active_locale())
}

fn human_readable_precise_duration_for_locale(duration: Duration, locale: Locale) -> String {
    let ms = duration.num_milliseconds() as f64;
    let weeks = ms / WEEK_TO_MS;
    if weeks >= 1. {
        return match locale {
            Locale::EnUs => String::from(">1 week"),
            Locale::ZhCn => String::from(">1 周"),
        };
    }
    let days = ms / DAY_TO_MS;
    if days >= 1. {
        return precise_quantity_with_unit(days, "days", "天", locale);
    }
    let hours = ms / HOUR_TO_MS;
    if hours >= 1. {
        return precise_quantity_with_unit(hours, "hours", "小时", locale);
    }
    let minutes = ms / MIN_TO_MS;
    if minutes >= 1. {
        return precise_quantity_with_unit(minutes, "min", "分钟", locale);
    }
    let seconds = ms / SEC_TO_MS;
    if seconds >= 1. {
        return precise_quantity_with_unit(seconds, "sec", "秒", locale);
    }
    match locale {
        Locale::EnUs => format!("{} ms", duration.num_milliseconds()),
        Locale::ZhCn => format!("{} 毫秒", duration.num_milliseconds()),
    }
}

fn precise_quantity_with_unit(
    num: f64,
    english_unit: &str,
    chinese_unit: &str,
    locale: Locale,
) -> String {
    let value = format_sigfigs(num, 3);
    match locale {
        Locale::EnUs => format!("{value} {english_unit}"),
        Locale::ZhCn => format!("{value} {chinese_unit}"),
    }
}

fn format_sigfigs(num: f64, sigfigs: usize) -> String {
    let a = num.abs();
    let precision = if a > 1. {
        let n = (1. + a.log10().floor()) as usize;
        sigfigs.saturating_sub(n)
    } else if a > 0. {
        let n = -(1. + a.log10().floor()) as usize;
        sigfigs + n
    } else {
        0
    };
    format!("{num:.precision$}")
}

/// Format an approximated duration into a human-readable string, e.g. "2 days ago".
/// Precision is limited to the most significant unit, i.e. 2 days and _n_ hours always displays
/// simply as "2 days ago".
pub fn human_readable_approx_duration(duration: Duration, sentence_case: bool) -> String {
    human_readable_approx_duration_for_locale(duration, sentence_case, warp_i18n::active_locale())
}

fn human_readable_approx_duration_for_locale(
    duration: Duration,
    sentence_case: bool,
    locale: Locale,
) -> String {
    let ms = duration.num_milliseconds() as f64;
    let years = ms / YEAR_TO_MS;
    if years >= 1. {
        return truncated_quantity_with_unit(years, "year", "年", locale);
    }
    let months = ms / MONTH_TO_MS;
    if months >= 1. {
        return truncated_quantity_with_unit(months, "month", "个月", locale);
    }
    let weeks = ms / WEEK_TO_MS;
    if weeks >= 1. {
        return truncated_quantity_with_unit(weeks, "week", "周", locale);
    }
    let days = ms / DAY_TO_MS;
    if days >= 1. {
        return truncated_quantity_with_unit(days, "day", "天", locale);
    }
    let hours = ms / HOUR_TO_MS;
    if hours >= 1. {
        return truncated_quantity_with_unit(hours, "hour", "小时", locale);
    }
    // Minutes and seconds are both abbreviated, so skip pluralization.
    let minutes = ms / MIN_TO_MS;
    if minutes >= 1. {
        return match locale {
            Locale::EnUs => format!("{} min ago", minutes as i32),
            Locale::ZhCn => format!("{} 分钟前", minutes as i32),
        };
    }
    match locale {
        Locale::EnUs if sentence_case => "Just now".to_owned(),
        Locale::EnUs => "just now".to_owned(),
        Locale::ZhCn => "刚刚".to_owned(),
    }
}

/// Provided a value and a unit, this will format the quantity as an integer number with the
/// unit pluralized if the value is not 1.
fn truncated_quantity_with_unit(
    num: f64,
    english_unit: &str,
    chinese_unit: &str,
    locale: Locale,
) -> String {
    let truncated_int = num as i32;
    match locale {
        Locale::EnUs if truncated_int == 1 => format!("{truncated_int} {english_unit} ago"),
        Locale::EnUs => format!("{truncated_int} {english_unit}s ago"),
        Locale::ZhCn => format!("{truncated_int} {chinese_unit}前"),
    }
}

/// Formats elapsed time as a whole-seconds string with proper singular/plural
/// (e.g. "1 second", "15 seconds").
pub fn format_elapsed_seconds(elapsed: StdDuration) -> String {
    format_elapsed_seconds_for_locale(elapsed, warp_i18n::active_locale())
}

fn format_elapsed_seconds_for_locale(elapsed: StdDuration, locale: Locale) -> String {
    let total_seconds = elapsed.as_secs();
    match locale {
        Locale::EnUs if total_seconds == 1 => "1 second".to_owned(),
        Locale::EnUs => format!("{total_seconds} seconds"),
        Locale::ZhCn => format!("{total_seconds} 秒"),
    }
}

/// Formats a monotonic `Instant` as a human-readable relative timestamp.
/// (Uses `Instant` rather than wall-clock `DateTime` for elapsed-time display.)
pub fn format_elapsed_since(created_at: instant::Instant) -> String {
    format_elapsed_since_for_locale(created_at, warp_i18n::active_locale())
}

fn format_elapsed_since_for_locale(created_at: instant::Instant, locale: Locale) -> String {
    let secs = created_at.elapsed().as_secs();

    if secs < 60 {
        match locale {
            Locale::EnUs => "Just now".to_string(),
            Locale::ZhCn => "刚刚".to_string(),
        }
    } else if secs < 3600 {
        let mins = secs / 60;
        match locale {
            Locale::EnUs if mins == 1 => "1 minute ago".to_string(),
            Locale::EnUs => format!("{mins} minutes ago"),
            Locale::ZhCn => format!("{mins} 分钟前"),
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        match locale {
            Locale::EnUs if hours == 1 => "1 hour ago".to_string(),
            Locale::EnUs => format!("{hours} hours ago"),
            Locale::ZhCn => format!("{hours} 小时前"),
        }
    } else {
        let days = secs / 86400;
        match locale {
            Locale::EnUs if days == 1 => "1 day ago".to_string(),
            Locale::EnUs => format!("{days} days ago"),
            Locale::ZhCn => format!("{days} 天前"),
        }
    }
}

#[cfg(test)]
#[path = "time_format_tests.rs"]
mod tests;
