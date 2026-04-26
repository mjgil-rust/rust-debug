// Basic tests for rust-debug

use rust_debug::{debug, error, info, warn, Level};

#[test]
fn test_level_ordering() {
    assert!(Level::Error < Level::Warn);
    assert!(Level::Warn < Level::Info);
    assert!(Level::Info < Level::Debug);
}

#[test]
fn test_level_display() {
    assert_eq!(Level::Error.to_string(), "ERROR");
    assert_eq!(Level::Warn.to_string(), "WARN");
    assert_eq!(Level::Info.to_string(), "INFO");
    assert_eq!(Level::Debug.to_string(), "DEBUG");
}

#[test]
fn test_macros_compile() {
    debug!("test", "debug message");
    info!("test", "info message");
    warn!("test", "warn message");
    error!("test", "error message");
}

#[test]
fn test_macros_with_formatting() {
    debug!("test", "value: {}", 42);
    info!("test", "x: {}, y: {}", 10, 20);
    warn!("test", "error: {:?}", "some error");
    error!("test", "hex: 0x{:x}", 255);
}
