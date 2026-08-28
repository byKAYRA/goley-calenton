

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const SHIM_CONFIG_ENV: &str = "GOLEY_SHIM_CONFIG";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShimMode {
    
    Run,
    
    CaptureWaits,
    
    DumpUnpacked,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShimConfig {
    
    pub mode: ShimMode,
    
    pub region: Option<String>,
    
    pub entry: Option<String>,
    
    pub loaded_event: Option<String>,
    
    pub ready_event: Option<String>,
    
    pub gameguard_ready_event: Option<String>,
    
    pub patches_path: Option<PathBuf>,
    
    pub log_path: PathBuf,
    
    pub verbosity: String,
    
    pub unpack: UnpackConfig,
    
    pub post_unpack_gate: Option<PostUnpackGateConfig>,
}

#[must_use]
pub fn verbosity_filter(level: u8) -> String {
    match level {
        0 => "info",
        1 => "goley_shim=debug,info",
        _ => "goley_shim=trace,debug",
    }
    .to_owned()
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnpackConfig {
    
    pub oep_rva: Option<u32>,
    
    pub poll_interval_ms: u64,
    
    pub stable_samples: u32,
    
    pub timeout_ms: u64,
    
    pub post_ready_delay_ms: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PostUnpackGateConfig {
    
    pub target_rva: u32,
    
    pub arrived_event: String,
    
    pub release_event: String,
    
    pub timeout_ms: u64,
}

impl ShimConfig {
    
    pub fn to_environment_value(&self) -> Result<OsString> {
        serde_json::to_string(self)
            .map(OsString::from)
            .context("failed to serialize shim configuration")
    }
}

pub fn handshake_event_names() -> (String, String) {
    let prefix = session_prefix();
    (format!("{prefix}-loaded"), format!("{prefix}-ready"))
}

pub fn post_unpack_gate_event_names() -> (String, String) {
    let prefix = session_prefix();
    (
        format!("{prefix}-post-unpack-arrived"),
        format!("{prefix}-post-unpack-release"),
    )
}

fn session_prefix() -> String {
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "Local\\GoleyBoot-{}-{tick:x}-{sequence:x}",
        std::process::id()
    )
}

pub fn temporary_log_path() -> PathBuf {
    let (_, unique) = handshake_event_names();
    let leaf = unique.replace("Local\\", "").replace(['\\', ':'], "-");
    env::temp_dir().join(format!("{leaf}.jsonl"))
}

pub fn resolve_shim_path(explicit: Option<&Path>) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit {
        candidates.push(path.to_path_buf());
    } else {
        if let Some(path) = env::var_os("GOLEY_SHIM_DLL") {
            candidates.push(PathBuf::from(path));
        }

        let executable = env::current_exe().context("failed to locate goley-boot executable")?;
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join("goley_shim.dll"));
            candidates.push(directory.join("goley-shim.dll"));
            if let Some(target_directory) = directory.parent() {
                candidates.push(target_directory.join("goley_shim.dll"));
                candidates.push(target_directory.join("goley-shim.dll"));
            }
        }
    }

    for candidate in &candidates {
        if candidate.is_file() {
            let canonical = candidate
                .canonicalize()
                .with_context(|| format!("failed to canonicalize {}", candidate.display()))?;
            ensure!(
                canonical
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("dll")),
                "shim path is not a DLL: {}",
                canonical.display()
            );
            return Ok(canonical);
        }
    }

    if explicit.is_some() {
        bail!(
            "explicit shim DLL was not found: {}",
            candidates
                .first()
                .map_or_else(|| "<missing>".into(), |path| path.display().to_string())
        );
    }

    let searched = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!("goley-shim.dll was not found; searched: {searched}")
}

pub fn resolve_patches_path(explicit: Option<&Path>, shim: &Path) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = explicit {
        candidates.push(path.to_path_buf());
    } else {
        if let Some(path) = env::var_os("GOLEY_PATCHES_TOML") {
            candidates.push(PathBuf::from(path));
        }
        if let Some(directory) = shim.parent() {
            candidates.push(directory.join("patches.toml"));
            candidates.push(directory.join("patches").join("patches.toml"));
        }
        if let Ok(current) = env::current_dir() {
            candidates.push(
                current
                    .join("crates")
                    .join("goley-shim")
                    .join("patches")
                    .join("patches.toml"),
            );
        }
    }

    for candidate in &candidates {
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .with_context(|| format!("failed to canonicalize {}", candidate.display()));
        }
    }

    let searched = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!("shim patch database was not found; searched: {searched}")
}

pub fn validate_region(region: &str) -> Result<()> {
    ensure!(!region.trim().is_empty(), "region must not be empty");
    ensure!(
        !region.contains(['\0', '\r', '\n']),
        "region contains a forbidden control character"
    );
    Ok(())
}

pub fn has_dll_extension(path: &OsStr) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_unpack_gate_contract_uses_exact_field_names() {
        let config = PostUnpackGateConfig {
            target_rva: 0x1234,
            arrived_event: "Local\\arrived".to_owned(),
            release_event: "Local\\release".to_owned(),
            timeout_ms: 45_000,
        };
        let value = serde_json::to_value(config).expect("gate config should serialize");
        assert_eq!(value["target_rva"], 0x1234);
        assert_eq!(value["arrived_event"], "Local\\arrived");
        assert_eq!(value["release_event"], "Local\\release");
        assert_eq!(value["timeout_ms"], 45_000);
    }

    #[test]
    fn debugger_gate_event_names_are_distinct() {
        let (arrived, release) = post_unpack_gate_event_names();
        assert_ne!(arrived, release);
        assert!(arrived.starts_with("Local\\GoleyBoot-"));
        assert!(release.starts_with("Local\\GoleyBoot-"));
    }
}
