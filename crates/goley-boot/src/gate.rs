

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, ensure};

use crate::cli::{PostUnpackGateArgs, PreResumeGateArgs};
use crate::config::{PostUnpackGateConfig, post_unpack_gate_event_names};

const SHIM_GATE_FAIL_OPEN_MARGIN_MS: u64 = 10_000;

#[derive(Clone, Debug)]
pub(crate) struct GateSpec {
    pub(crate) release_path: PathBuf,
    pub(crate) metadata_path: PathBuf,
    pub(crate) timeout: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct PostUnpackGateSpec {
    pub(crate) release_path: PathBuf,
    pub(crate) metadata_path: PathBuf,
    pub(crate) timeout: Duration,
    pub(crate) config: PostUnpackGateConfig,
}

pub(crate) fn prepare(args: &PreResumeGateArgs) -> Result<Option<GateSpec>> {
    let Some(configured_path) = args.pre_resume_gate.as_deref() else {
        return Ok(None);
    };
    ensure!(
        args.pre_resume_gate_timeout > 0,
        "pre-resume-gate-timeout must be greater than zero"
    );

    let release_path = absolute_path(configured_path)?;
    let metadata_path = metadata_path(&release_path);
    ensure!(
        !release_path.exists(),
        "pre-resume gate release file already exists: {}; remove it or choose a fresh path",
        release_path.display()
    );
    ensure!(
        !metadata_path.exists(),
        "pre-resume gate metadata already exists: {}; remove it or choose a fresh path",
        metadata_path.display()
    );
    let parent = release_path
        .parent()
        .context("pre-resume gate path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create pre-resume gate directory {}",
            parent.display()
        )
    })?;

    Ok(Some(GateSpec {
        release_path,
        metadata_path,
        timeout: Duration::from_secs(args.pre_resume_gate_timeout),
    }))
}

pub(crate) fn prepare_post_unpack(args: &PostUnpackGateArgs) -> Result<Option<PostUnpackGateSpec>> {
    let configured = args.post_unpack_gate.as_deref();
    let target_rva = args.post_unpack_gate_rva;
    ensure!(
        configured.is_some() == target_rva.is_some(),
        "--post-unpack-gate and --post-unpack-gate-rva must be supplied together"
    );
    let (Some(configured_path), Some(target_rva)) = (configured, target_rva) else {
        return Ok(None);
    };
    ensure!(
        args.post_unpack_gate_timeout > 0,
        "post-unpack-gate-timeout must be greater than zero"
    );
    ensure!(target_rva != 0, "post-unpack-gate-rva must not be zero");

    let release_path = absolute_path(configured_path)?;
    let metadata_path = metadata_path(&release_path);
    validate_fresh_paths(&release_path, &metadata_path, "post-unpack")?;
    let (arrived_event, release_event) = post_unpack_gate_event_names();
    let timeout = Duration::from_secs(args.post_unpack_gate_timeout);
    let launcher_timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
    let timeout_ms = launcher_timeout_ms
        .checked_add(SHIM_GATE_FAIL_OPEN_MARGIN_MS)
        .context("post-unpack shim timeout overflow")?;
    ensure!(
        timeout_ms <= (i64::MAX as u64) / 10_000,
        "post-unpack-gate-timeout is too large for a finite NT relative timeout"
    );
    Ok(Some(PostUnpackGateSpec {
        release_path,
        metadata_path,
        timeout,
        config: PostUnpackGateConfig {
            target_rva,
            arrived_event,
            release_event,
            timeout_ms,
        },
    }))
}

fn validate_fresh_paths(release_path: &Path, metadata_path: &Path, label: &str) -> Result<()> {
    ensure_path_absent(release_path, label, "release file")?;
    ensure_path_absent(metadata_path, label, "metadata")?;
    let parent = release_path
        .parent()
        .with_context(|| format!("{label} gate path has no parent directory"))?;
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create {label} gate directory {}",
            parent.display()
        )
    })?;
    Ok(())
}

fn ensure_path_absent(path: &Path, label: &str, kind: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => anyhow::bail!(
            "{label} gate {kind} already exists: {}; remove it or choose a fresh path",
            path.display()
        ),
        Err(error) => Err(error)
            .with_context(|| format!("failed to inspect {label} gate {kind} {}", path.display())),
    }
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(std::env::current_dir()
        .context("failed to resolve current directory for debugger gate")?
        .join(path))
}

fn metadata_path(release_path: &Path) -> PathBuf {
    let mut path = OsString::from(release_path.as_os_str());
    path.push(".metadata.json");
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_a_companion_not_the_release_marker() {
        assert_eq!(
            metadata_path(Path::new(r"C:\Temp\goley.resume")),
            PathBuf::from(r"C:\Temp\goley.resume.metadata.json")
        );
    }

    #[test]
    fn post_unpack_gate_requires_path_and_rva_as_a_pair() {
        let args = PostUnpackGateArgs {
            post_unpack_gate: Some(PathBuf::from(r"C:\Temp\late.release")),
            post_unpack_gate_rva: None,
            post_unpack_gate_timeout: 120,
        };
        let error = prepare_post_unpack(&args).expect_err("an incomplete gate must fail");
        assert!(error.to_string().contains("supplied together"));
    }
}
