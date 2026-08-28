

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bin=goley-boot=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=goley-boot=/MANIFESTUAC:level='requireAdministrator' uiAccess='false'"
        );
    }
}
