use super::Channel;

#[test]
fn oss_is_terminal_only() {
    assert!(!Channel::Oss.allows_ai());
    assert!(!Channel::Oss.allows_account_login());
    assert!(Channel::Stable.allows_ai());
    assert!(Channel::Stable.allows_account_login());
    assert!(Channel::Preview.allows_ai());
    assert!(Channel::Dev.allows_ai());
    assert!(Channel::Local.allows_ai());
    assert!(Channel::Integration.allows_ai());
}
