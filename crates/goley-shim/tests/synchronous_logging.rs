use std::{fs, io::Read};

#[test]
fn wait_enter_style_record_is_visible_without_guard_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.jsonl");
    let _guard = goley_shim::logging::init(&path, "info").unwrap();

    tracing::info!(
        event_type = "kernel_wait",
        operation = "wait_enter",
        outcome = "pending",
        api = "WaitForSingleObject",
        "synchronous visibility fixture"
    );

    let mut file = fs::File::open(&path).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    assert!(contents.contains("wait_enter"));
    assert!(contents.contains("WaitForSingleObject"));
}
