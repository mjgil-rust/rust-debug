#![cfg(feature = "tracing-integration")]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("rust-debug-{label}-{}-{nanos}", std::process::id()))
}

#[test]
fn init_tracing_returns_error_when_a_global_subscriber_already_exists() {
    let log_root = unique_temp_dir("existing-subscriber");
    std::fs::create_dir_all(&log_root).unwrap();

    std::env::set_var("XDG_STATE_HOME", &log_root);
    std::env::remove_var("DEBUG");
    std::env::remove_var("DEBUG_FILE");
    std::env::remove_var("DEBUG_LOG_DIR");

    tracing::subscriber::set_global_default(tracing_subscriber::registry()).unwrap();

    match rust_debug::init_tracing("existing-subscriber") {
        Ok(_) => panic!("expected init_tracing to fail when a global subscriber already exists"),
        Err(err) => assert_eq!(err.kind(), std::io::ErrorKind::Other),
    }
}
