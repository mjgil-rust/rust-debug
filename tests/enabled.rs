use rust_debug::{enabled, enabled_for, Level};

#[test]
fn enabled_ignores_debug_level_but_respects_namespace_patterns() {
    std::env::set_var("DEBUG", "app");
    std::env::set_var("DEBUG_LEVEL", "info");
    std::env::remove_var("DEBUG_FILE");
    std::env::remove_var("DEBUG_LOG_DIR");

    assert!(enabled("app"));
    assert!(enabled_for("app", Level::Info));
    assert!(!enabled_for("app", Level::Debug));
    assert!(!enabled("other"));
}
