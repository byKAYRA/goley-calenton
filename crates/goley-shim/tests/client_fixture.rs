

use std::path::PathBuf;

#[test]
fn optional_client_fixture_is_never_required_for_unit_tests() {
    let Some(root) = std::env::var_os("GOLEY_CLIENT_DIR").map(PathBuf::from) else {
        eprintln!("skipped: GOLEY_CLIENT_DIR is not configured");
        return;
    };
    let executable = root.join("Goley.exe");
    assert!(
        executable.is_file(),
        "GOLEY_CLIENT_DIR was set but {} is missing",
        executable.display()
    );
}
