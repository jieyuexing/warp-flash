use settings::Setting;

use super::QuitOnLastWindowClosed;

#[test]
fn warposs_closes_the_last_window_by_terminating_by_default() {
    assert!(QuitOnLastWindowClosed::default_value());
}
