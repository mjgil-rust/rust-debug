#![cfg(feature = "tracing-integration")]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_file(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("rust-debug-{label}-{}-{nanos}", std::process::id()))
}

#[test]
fn init_tracing_propagates_file_logger_errors() {
    let xdg_state_file = unique_temp_file("xdg-state-file");
    std::fs::write(&xdg_state_file, b"not a directory").unwrap();

    std::env::set_var("XDG_STATE_HOME", &xdg_state_file);
    std::env::remove_var("DEBUG");
    std::env::remove_var("DEBUG_FILE");
    std::env::remove_var("DEBUG_LOG_DIR");

    match rust_debug::init_tracing("file-error") {
        Ok(_) => panic!("expected init_tracing to propagate the file logger error"),
        Err(err) => assert_eq!(err.kind(), std::io::ErrorKind::NotADirectory),
    }
}
