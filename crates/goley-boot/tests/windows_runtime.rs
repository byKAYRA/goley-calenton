

#[cfg(windows)]
#[test]
fn configured_client_is_an_x86_pe_image() {
    let Some(directory) = std::env::var_os("GOLEY_CLIENT_DIR") else {
        eprintln!("skipped: GOLEY_CLIENT_DIR is not set");
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let client = [
        directory.join("BinaryTr").join("BinaryTr.bin"),
        directory.join("Goley_.exe"),
        directory.join("Goley.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .expect("GOLEY_CLIENT_DIR does not contain BinaryTr\\BinaryTr.bin, Goley_.exe, or Goley.exe");

    let info = goley_boot::pe::require_x86_client(&client)
        .expect("configured client should be a valid x86 PE32 image");
    assert!(info.is_x86_pe32());
}

#[cfg(not(windows))]
#[test]
fn windows_runtime_probe_is_skipped() {
    eprintln!("skipped: goley-boot runtime probe requires Windows");
}
