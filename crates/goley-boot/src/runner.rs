

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

use crate::cli::{BootCommand, CaptureWaitsArgs, Cli, DumpUnpackedArgs, RunArgs};
use crate::config::{
    ShimConfig, ShimMode, UnpackConfig, handshake_event_names, resolve_patches_path,
    resolve_shim_path, temporary_log_path, validate_region, verbosity_filter,
};
use crate::dump::{StabilityTracker, preflight_destination, write_dump};
use crate::gate;
use crate::pe;
use crate::report::{parse_capture_log, write_report};

pub fn execute(cli: Cli) -> Result<()> {
    initialize_tracing();
    match cli.command {
        BootCommand::Run(args) => execute_run(args),
        BootCommand::CaptureWaits(args) => execute_capture(args),
        BootCommand::DumpUnpacked(args) => execute_dump(args),
    }
}

fn execute_dump(args: DumpUnpackedArgs) -> Result<()> {
    validate_launch_controls(&args.launch)?;
    validate_region(&args.region)?;
    ensure!(
        args.launch.patches.is_none(),
        "dump-unpacked captures a pristine mapped image and does not accept --patches"
    );
    ensure!(
        args.snapshot_interval_ms > 0,
        "snapshot-interval-ms must be greater than zero"
    );
    ensure!(
        args.quiescence_ms >= args.snapshot_interval_ms,
        "quiescence-ms must be at least snapshot-interval-ms"
    );
    ensure!(
        args.quiescence_ms < args.launch.timeout.saturating_mul(1_000),
        "quiescence-ms must be shorter than the command timeout"
    );
    let destination = preflight_destination(&args.out)?;
    pe::require_x86_client(&args.launch.client)?;
    let shim = resolve_shim_path(args.launch.shim.as_deref())?;
    let (loaded_event, ready_event) = handshake_event_names();
    let log_path = temporary_log_path();
    prepare_empty_log(&log_path)?;
    let config = ShimConfig {
        mode: ShimMode::DumpUnpacked,
        region: Some(args.region),
        entry: None,
        loaded_event: Some(loaded_event),
        ready_event: Some(ready_event),
        gameguard_ready_event: None,
        patches_path: None,
        log_path: log_path.clone(),
        verbosity: verbosity_filter(args.launch.verbose),
        unpack: UnpackConfig {
            oep_rva: args.launch.oep_rva,
            poll_interval_ms: args.launch.unpack_poll_ms,
            stable_samples: args.launch.unpack_stable_samples,
            timeout_ms: args.launch.timeout.saturating_mul(1_000),
            post_ready_delay_ms: args.launch.late_inject_ms,
        },
        post_unpack_gate: None,
    };

    #[cfg(windows)]
    {
        let timeout = Duration::from_secs(args.launch.timeout);
        let interval = Duration::from_millis(args.snapshot_interval_ms);
        let quiescence = Duration::from_millis(args.quiescence_ms);
        let (mut session, baseline, started) =
            crate::windows_process::launch_for_dump(&args.launch.client, &shim, &config, timeout)?;
        let baseline_sha256 = baseline.sha256();
        info!(
            pid = session.process_id(),
            executable_ranges = baseline.ranges.len(),
            code_sha256 = %baseline_sha256,
            "captured pre-resume main-image baseline"
        );
        let mut tracker = StabilityTracker::new(baseline)?;

        let operation = (|| -> Result<_> {
            loop {
                let elapsed = started.elapsed();
                ensure!(
                    elapsed < timeout,
                    "unpack measurement timed out after {} ms (transition_seen={}, change_samples={}, maximum_changed_ranges={})",
                    elapsed.as_millis(),
                    tracker.transition_seen(),
                    tracker.change_samples(),
                    tracker.maximum_changed_ranges()
                );
                if let Some(exit_code) = session.wait_for_exit(Some(Duration::ZERO))? {
                    anyhow::bail!(
                        "client exited with code 0x{exit_code:08x} before the executable image became quiescent"
                    );
                }
                thread::sleep(interval.min(timeout.saturating_sub(elapsed)));
                let sample = session.capture_code_snapshot().with_context(|| {
                    format!(
                        "failed to capture executable-page sample at {} ms",
                        started.elapsed().as_millis()
                    )
                })?;
                let sample_sha256 = sample.sha256();
                let ready = tracker.observe(sample, started.elapsed(), quiescence);
                debug!(
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    code_sha256 = %sample_sha256,
                    transition_seen = tracker.transition_seen(),
                    change_samples = tracker.change_samples(),
                    "sampled main-image executable pages"
                );
                if !ready {
                    continue;
                }

let mapped = match session.capture_mapped_image() {
                    Ok(mapped) => mapped,
                    Err(error) => {
                        warn!(error = %error, "full mapped-image read raced a protection change; retrying");
                        continue;
                    }
                };
                let after = session.capture_code_snapshot()?;
                if &mapped.code != tracker.last() || after != mapped.code {
                    let _ = tracker.observe(after, started.elapsed(), quiescence);
                    debug!(
                        "mapped image changed during capture; continuing quiescence measurement"
                    );
                    continue;
                }

                let final_code_sha256 = mapped.code.sha256();
                let readable_bytes = mapped.readable_bytes;
                let zero_filled_bytes = mapped.zero_filled_bytes;
                let result = write_dump(&destination, mapped)?;
                return Ok((
                    result,
                    final_code_sha256,
                    readable_bytes,
                    zero_filled_bytes,
                    started.elapsed(),
                    tracker.first_transition(),
                    tracker.change_samples(),
                    tracker.maximum_changed_ranges(),
                ));
            }
        })();

        let cleanup = match session.wait_for_exit(Some(Duration::ZERO)) {
            Ok(None) => session.terminate(0x474c_5944),
            Ok(Some(_)) => Ok(()),
            Err(error) => Err(error),
        };
        let (
            result,
            final_code_sha256,
            readable_bytes,
            zero_filled_bytes,
            elapsed,
            first_transition,
            change_samples,
            maximum_changed_ranges,
        ) = operation?;
        cleanup.context("dump succeeded, but stopping the observed child failed")?;

        info!(
            output = %result.path.display(),
            sha256 = %result.sha256,
            bytes = result.size,
            elapsed_ms = elapsed.as_millis() as u64,
            "post-unpack image written"
        );
        println!("output={}", result.path.display());
        println!("sha256={}", result.sha256);
        println!("size={}", result.size);
        println!("image_base=0x{:08X}", result.rewrite.captured_image_base);
        println!(
            "original_image_base=0x{:08X}",
            result.rewrite.original_image_base
        );
        println!("section_count={}", result.rewrite.section_count);
        println!(
            "synthesized_sections={}",
            result.rewrite.synthesized_sections
        );
        println!("baseline_code_sha256={baseline_sha256}");
        println!("final_code_sha256={final_code_sha256}");
        println!(
            "first_transition_after_resume_ms={}",
            first_transition.map_or(0, |duration| duration.as_millis())
        );
        println!("quiescence_ms={}", args.quiescence_ms);
        println!("capture_elapsed_after_resume_ms={}", elapsed.as_millis());
        println!("change_samples={change_samples}");
        println!("maximum_changed_ranges={maximum_changed_ranges}");
        println!("readable_bytes={readable_bytes}");
        println!("zero_filled_bytes={zero_filled_bytes}");
        println!("shim_log={}", log_path.display());
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (shim, config, destination, log_path);
        anyhow::bail!("goley-boot process launch is available only on Windows")
    }
}

fn execute_run(args: RunArgs) -> Result<()> {
    validate_launch_controls(&args.launch)?;
    validate_region(&args.region)?;
    pe::require_x86_client(&args.launch.client)?;
    let shim = resolve_shim_path(args.launch.shim.as_deref())?;
    let patches = resolve_patches_path(args.launch.patches.as_deref(), &shim)?;
    let (loaded_event, ready_event) = handshake_event_names();
    let log_path = temporary_log_path();
    let post_unpack_gate = gate::prepare_post_unpack(&args.post_unpack)?;
    let config = ShimConfig {
        mode: ShimMode::Run,
        region: Some(args.region),
        entry: args.entry.map(|endpoint| endpoint.to_string()),
        loaded_event: Some(loaded_event),
        ready_event: Some(ready_event),
        gameguard_ready_event: args.gameguard_ready_event,
        patches_path: Some(patches),
        log_path: log_path.clone(),
        verbosity: verbosity_filter(args.launch.verbose),
        unpack: UnpackConfig {
            oep_rva: args.launch.oep_rva,
            poll_interval_ms: args.launch.unpack_poll_ms,
            stable_samples: args.launch.unpack_stable_samples,
            timeout_ms: args.launch.timeout.saturating_mul(1_000),
            post_ready_delay_ms: args.launch.late_inject_ms,
        },
        post_unpack_gate: post_unpack_gate.as_ref().map(|gate| gate.config.clone()),
    };

    #[cfg(windows)]
    {
        let pre_resume_gate = gate::prepare(&args.pre_resume)?;
        let mut session = crate::windows_process::launch(
            &args.launch.client,
            &shim,
            &config,
            args.runparam_key.as_deref(),
            Duration::from_secs(args.launch.timeout),
            pre_resume_gate.as_ref(),
            post_unpack_gate.as_ref(),
        )?;
        info!(
            pid = session.process_id(),
            "shim ready; observing client until exit"
        );
        if args.detach {
            info!(
                pid = session.process_id(),
                log = %log_path.display(),
                "detaching after READY; client remains running"
            );
            return Ok(());
        }
        let exit_code = session
            .wait_for_exit(None)?
            .context("infinite process wait unexpectedly timed out")?;
        info!(exit_code, log = %log_path.display(), "client exited");
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (shim, config, log_path, args.pre_resume, post_unpack_gate);
        anyhow::bail!("goley-boot process launch is available only on Windows")
    }
}

fn execute_capture(args: CaptureWaitsArgs) -> Result<()> {
    validate_launch_controls(&args.launch)?;
    validate_region(&args.region)?;
    pe::require_x86_client(&args.launch.client)?;
    let shim = resolve_shim_path(args.launch.shim.as_deref())?;
    let patches = resolve_patches_path(args.launch.patches.as_deref(), &shim)?;
    let (loaded_event, ready_event) = handshake_event_names();
    let log_path = args.log.unwrap_or_else(temporary_log_path);
    prepare_empty_log(&log_path)?;
    let report_path = args
        .report
        .unwrap_or_else(|| PathBuf::from("goley-wait-report.md"));
    let post_unpack_gate = gate::prepare_post_unpack(&args.post_unpack)?;
    let config = ShimConfig {
        mode: ShimMode::CaptureWaits,
        region: Some(args.region),
        entry: None,
        loaded_event: Some(loaded_event),
        ready_event: Some(ready_event),
        gameguard_ready_event: None,
        patches_path: Some(patches),
        log_path: log_path.clone(),
        verbosity: verbosity_filter(args.launch.verbose),
        unpack: UnpackConfig {
            oep_rva: args.launch.oep_rva,
            poll_interval_ms: args.launch.unpack_poll_ms,
            stable_samples: args.launch.unpack_stable_samples,
            timeout_ms: args.launch.timeout.saturating_mul(1_000),
            post_ready_delay_ms: args.launch.late_inject_ms,
        },
        post_unpack_gate: post_unpack_gate.as_ref().map(|gate| gate.config.clone()),
    };

    #[cfg(windows)]
    {
        let timeout = Duration::from_secs(args.launch.timeout);
        let pre_resume_gate = gate::prepare(&args.pre_resume)?;
        let mut session = match crate::windows_process::launch(
            &args.launch.client,
            &shim,
            &config,
            args.runparam_key.as_deref(),
            timeout,
            pre_resume_gate.as_ref(),
            post_unpack_gate.as_ref(),
        ) {
            Ok(session) => session,
            Err(error) => {

let report = parse_capture_log(&log_path)?;
                write_report(&report_path, &report)?;
                return Err(error).with_context(|| {
                    format!(
                        "capture handshake failed; partial report written to {}",
                        report_path.display()
                    )
                });
            }
        };
        info!(pid = session.process_id(), "capturing startup wait objects");
        if session.wait_for_exit(Some(timeout))?.is_none() {
            warn!(
                pid = session.process_id(),
                "capture deadline reached; stopping observed child"
            );
            session.terminate(0x474c_5943)?;
        }
        let report = parse_capture_log(&log_path)?;
        write_report(&report_path, &report)?;
        info!(
            report = %report_path.display(),
            raw_log = %log_path.display(),
            records = report.parsed_records,
            "wait-object report written"
        );
        println!("{}", report_path.display());
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = (shim, config, report_path, args.pre_resume, post_unpack_gate);
        anyhow::bail!("goley-boot process launch is available only on Windows")
    }
}

fn validate_launch_controls(args: &crate::cli::LaunchArgs) -> Result<()> {
    ensure!(args.timeout > 0, "timeout must be greater than zero");
    ensure!(
        args.unpack_poll_ms > 0,
        "unpack-poll-ms must be greater than zero"
    );
    ensure!(
        args.unpack_stable_samples > 0,
        "unpack-stable-samples must be greater than zero"
    );
    Ok(())
}

fn prepare_empty_log(path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory {}", parent.display()))?;
    }
    fs::write(path, "").with_context(|| format!("failed to prepare log {}", path.display()))
}

fn initialize_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
