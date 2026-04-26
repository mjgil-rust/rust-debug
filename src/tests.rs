use super::*;
use crate::formatting::days_to_ymd;

#[test]
fn test_pattern_matches() {
    assert!(pattern_matches("*", "anything"));
    assert!(pattern_matches("vnc", "vnc"));
    assert!(!pattern_matches("vnc", "clipboard"));
    assert!(pattern_matches("vnc:*", "vnc:conn"));
    assert!(pattern_matches("vnc:*", "vnc:"));
    assert!(!pattern_matches("vnc:*", "clipboard"));
}

#[test]
fn test_parse_debug_patterns() {
    let (inc, exc) = parse_debug_patterns("vnc:*,clipboard,-key");
    assert_eq!(inc, vec!["vnc:*", "clipboard"]);
    assert_eq!(exc, vec!["key"]);
}

#[test]
fn test_parse_debug_patterns_spaces() {
    let (inc, exc) = parse_debug_patterns("vnc clipboard -key");
    assert_eq!(inc, vec!["vnc", "clipboard"]);
    assert_eq!(exc, vec!["key"]);
}

#[test]
fn test_humanize() {
    assert_eq!(humanize(0), "0us");
    assert_eq!(humanize(500), "500us");
    assert_eq!(humanize(1_500), "1.5ms");
    assert_eq!(humanize(1_500_000), "1.5s");
    assert_eq!(humanize(90_000_000), "1.5m");
    assert_eq!(humanize(5_400_000_000), "1.5h");
}

#[test]
fn test_days_to_ymd() {
    assert_eq!(days_to_ymd(0), (1970, 1, 1));
    assert_eq!(days_to_ymd(10957), (2000, 1, 1));
    assert_eq!(days_to_ymd(20527), (2026, 3, 15));
}

#[test]
fn test_level_ordering() {
    assert!(Level::Error < Level::Warn);
    assert!(Level::Warn < Level::Info);
    assert!(Level::Info < Level::Debug);
}
