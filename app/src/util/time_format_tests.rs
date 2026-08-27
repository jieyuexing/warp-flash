use super::*;

#[test]
fn test_format_sigfigs() {
    assert_eq!(format_sigfigs(0.000456, 2,), "0.00046");
    assert_eq!(format_sigfigs(0.043256, 3,), "0.0433");
    assert_eq!(format_sigfigs(0.01, 2,), "0.010");
    assert_eq!(format_sigfigs(10., 3,), "10.0");
    assert_eq!(format_sigfigs(456.719, 4,), "456.7");
    assert_eq!(format_sigfigs(10., 2,), "10");
}

#[test]
fn test_human_readable_precise_duration() {
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::milliseconds(3), Locale::EnUs),
        "3 ms".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::milliseconds(10), Locale::EnUs),
        "10 ms".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::milliseconds(3141), Locale::EnUs),
        "3.14 sec".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::milliseconds(19961), Locale::EnUs),
        "20.0 sec".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::seconds(61), Locale::EnUs),
        "1.02 min".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::minutes(930), Locale::EnUs),
        "15.5 hours".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::hours(46), Locale::EnUs),
        "1.92 days".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::weeks(2), Locale::EnUs),
        ">1 week".to_owned()
    );

    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::milliseconds(3141), Locale::ZhCn),
        "3.14 秒"
    );
    assert_eq!(
        human_readable_precise_duration_for_locale(Duration::hours(46), Locale::ZhCn),
        "1.92 天"
    );
}

#[test]
fn format_elapsed_seconds_pluralizes_and_truncates() {
    assert_eq!(
        format_elapsed_seconds_for_locale(StdDuration::from_secs(0), Locale::EnUs),
        "0 seconds"
    );
    assert_eq!(
        format_elapsed_seconds_for_locale(StdDuration::from_secs(1), Locale::EnUs),
        "1 second"
    );
    assert_eq!(
        format_elapsed_seconds_for_locale(StdDuration::from_secs(15), Locale::EnUs),
        "15 seconds"
    );
    // Subsecond precision is truncated, not rounded.
    assert_eq!(
        format_elapsed_seconds_for_locale(StdDuration::from_millis(1999), Locale::EnUs),
        "1 second"
    );
    assert_eq!(
        format_elapsed_seconds_for_locale(StdDuration::from_secs(15), Locale::ZhCn),
        "15 秒"
    );
}

#[test]
fn test_human_readable_approx_duration() {
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::milliseconds(2), false, Locale::EnUs,),
        "just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::seconds(2), false, Locale::EnUs),
        "just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::milliseconds(2), true, Locale::EnUs,),
        "Just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::seconds(2), true, Locale::EnUs),
        "Just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::seconds(90), false, Locale::EnUs),
        "1 min ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::minutes(100), false, Locale::EnUs),
        "1 hour ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::minutes(130), false, Locale::EnUs),
        "2 hours ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::days(4), false, Locale::EnUs),
        "4 days ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::weeks(1), false, Locale::EnUs),
        "1 week ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::weeks(15), false, Locale::EnUs),
        "3 months ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::weeks(520), false, Locale::EnUs),
        "9 years ago".to_owned()
    );

    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::milliseconds(2), true, Locale::ZhCn,),
        "刚刚"
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::minutes(130), false, Locale::ZhCn),
        "2 小时前"
    );
    assert_eq!(
        human_readable_approx_duration_for_locale(Duration::weeks(15), false, Locale::ZhCn),
        "3 个月前"
    );
}
