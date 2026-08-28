

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString, c_void};
use std::fs;
use std::io::{self, Write};
use std::mem::size_of;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::Serialize;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_BAD_LENGTH, ERROR_ELEVATION_REQUIRED,
    ERROR_NO_MORE_FILES, GetLastError, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
#[cfg(target_arch = "x86")]
use windows::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_X86, CONTEXT_DEBUG_REGISTERS_X86, GetThreadContext, SetThreadContext,
};
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE,
    TH32CS_SNAPMODULE32,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE,
    PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS,
    PAGE_READWRITE, PAGE_WRITECOPY, VirtualAllocEx, VirtualFreeEx, VirtualQueryEx,
};
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW, CreateRemoteThread,
    GetExitCodeProcess, GetExitCodeThread, LPTHREAD_START_ROUTINE, PROCESS_INFORMATION,
    ResumeThread, STARTUPINFOW, SetEvent, TerminateProcess, WaitForMultipleObjects,
    WaitForSingleObject,
};
use windows::core::{PCSTR, PCWSTR, PWSTR};

use crate::config::{SHIM_CONFIG_ENV, ShimConfig};
use crate::dump::{CodeSnapshot, ExecutableRange, MappedImage, MemoryRange};
use crate::gate::{GateSpec, PostUnpackGateSpec};

const LOAD_LIBRARY_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_MAIN_IMAGE_SIZE: usize = 512 * 1024 * 1024;
const REMOTE_READ_CHUNK: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
struct RemoteImageLayout {
    base: usize,
    size: usize,
}

struct LaunchOptions<'a> {
    runparam_key: Option<&'a str>,
    capture_baseline: bool,
    pre_resume_gate: Option<&'a GateSpec>,
    post_unpack_gate: Option<&'a PostUnpackGateSpec>,
}

pub(crate) struct BootSession {
    process: OwnedHandle,
    _primary_thread: OwnedHandle,
    process_id: u32,
    main_image: Option<RemoteImageLayout>,
}

impl BootSession {
    
    pub(crate) const fn process_id(&self) -> u32 {
        self.process_id
    }

pub(crate) fn wait_for_exit(&mut self, timeout: Option<Duration>) -> Result<Option<u32>> {
        let wait_ms = timeout.map_or(u32::MAX, duration_ms);
        let status = unsafe { WaitForSingleObject(self.process.raw(), wait_ms) };
        if status == WAIT_OBJECT_0 {
            return self.exit_code().map(Some);
        }
        if status == WAIT_TIMEOUT {
            return Ok(None);
        }
        if status == WAIT_FAILED {
            return Err(windows::core::Error::from_thread())
                .context("WaitForSingleObject(process) failed");
        }
        bail!("unexpected process wait status 0x{:08x}", status.0)
    }

pub(crate) fn terminate(&mut self, exit_code: u32) -> Result<()> {
        unsafe { TerminateProcess(self.process.raw(), exit_code) }
            .context("TerminateProcess(observed child) failed")?;
        let _ = unsafe { WaitForSingleObject(self.process.raw(), 5_000) };
        Ok(())
    }

pub(crate) fn capture_code_snapshot(&self) -> Result<CodeSnapshot> {
        let layout = self
            .main_image
            .context("this boot session has no dump image layout")?;
        capture_code_snapshot(self.process.raw(), layout)
    }

pub(crate) fn capture_mapped_image(&self) -> Result<MappedImage> {
        let layout = self
            .main_image
            .context("this boot session has no dump image layout")?;
        capture_mapped_image(self.process.raw(), layout)
    }

    fn exit_code(&self) -> Result<u32> {
        let mut code = 0;
        unsafe { GetExitCodeProcess(self.process.raw(), &mut code) }
            .context("GetExitCodeProcess failed")?;
        Ok(code)
    }
}

pub(crate) fn launch(
    client: &Path,
    shim: &Path,
    config: &ShimConfig,
    runparam_key: Option<&str>,
    handshake_timeout: Duration,
    pre_resume_gate: Option<&GateSpec>,
    post_unpack_gate: Option<&PostUnpackGateSpec>,
) -> Result<BootSession> {
    let (session, baseline, resumed_at) = launch_internal(
        client,
        shim,
        config,
        handshake_timeout,
        LaunchOptions {
            runparam_key,
            capture_baseline: false,
            pre_resume_gate,
            post_unpack_gate,
        },
    )?;
    debug_assert!(baseline.is_none());
    debug_assert!(resumed_at.is_some());
    Ok(session)
}

pub(crate) fn launch_for_dump(
    client: &Path,
    shim: &Path,
    config: &ShimConfig,
    handshake_timeout: Duration,
) -> Result<(BootSession, CodeSnapshot, Instant)> {
    let (session, baseline, resumed_at) = launch_internal(
        client,
        shim,
        config,
        handshake_timeout,
        LaunchOptions {
            runparam_key: None,
            capture_baseline: true,
            pre_resume_gate: None,
            post_unpack_gate: None,
        },
    )?;
    Ok((
        session,
        baseline.context("dump launch did not capture a pre-resume baseline")?,
        resumed_at.context("dump launch did not record primary-thread resume time")?,
    ))
}

fn launch_internal(
    client: &Path,
    shim: &Path,
    config: &ShimConfig,
    handshake_timeout: Duration,
    options: LaunchOptions<'_>,
) -> Result<(BootSession, Option<CodeSnapshot>, Option<Instant>)> {
    let LaunchOptions {
        runparam_key,
        capture_baseline,
        pre_resume_gate,
        post_unpack_gate,
    } = options;
    ensure!(
        size_of::<usize>() == 4,
        "goley-boot must itself be built for i686-pc-windows-msvc before injecting an x86 client"
    );
    let client = client
        .canonicalize()
        .with_context(|| format!("failed to canonicalize client {}", client.display()))?;
    let shim = shim
        .canonicalize()
        .with_context(|| format!("failed to canonicalize shim {}", shim.display()))?;
    let loaded_name = config
        .loaded_event
        .as_deref()
        .context("shim configuration has no loaded_event")?;
    let ready_name = config
        .ready_event
        .as_deref()
        .context("shim configuration has no ready_event")?;
    let region = config
        .region
        .as_deref()
        .context("shim configuration has no client region argument")?;
    match (&config.post_unpack_gate, post_unpack_gate) {
        (None, None) => {}
        (Some(configured), Some(gate)) => ensure!(
            configured == &gate.config,
            "post-unpack gate configuration does not match the prepared launcher gate"
        ),
        _ => {
            bail!("post-unpack gate must be present in both launcher state and shim configuration")
        }
    }

    let loaded_event = create_named_event(loaded_name)
        .with_context(|| format!("failed to create loaded event {loaded_name}"))?;
    let ready_event = create_named_event(ready_name)
        .with_context(|| format!("failed to create ready event {ready_name}"))?;
    let post_unpack_events = if let Some(gate) = post_unpack_gate {
        Some((
            create_named_event(&gate.config.arrived_event).with_context(|| {
                format!(
                    "failed to create post-unpack arrived event {}",
                    gate.config.arrived_event
                )
            })?,
            create_named_event(&gate.config.release_event).with_context(|| {
                format!(
                    "failed to create post-unpack release event {}",
                    gate.config.release_event
                )
            })?,
        ))
    } else {
        None
    };
    let environment = unicode_environment(config)?;
    let (process, primary_thread, process_id, primary_thread_id) =
        create_suspended_process(&client, region, runparam_key, &environment)?;

    let mut main_image = None;
    let mut baseline = None;
    let mut resumed_at = None;
    let setup = (|| -> Result<()> {
        inject_load_library(process.raw(), &shim)?;
        wait_for_stage(
            loaded_event.raw(),
            process.raw(),
            handshake_timeout,
            "shim LOADED",
        )?;

        let mut post_unpack_arm = None;
        if capture_baseline || pre_resume_gate.is_some() || post_unpack_gate.is_some() {
            let layout = locate_main_image(process.raw(), process_id, &client)?;
            if capture_baseline {
                let snapshot = capture_code_snapshot(process.raw(), layout)
                    .context("failed to capture pre-resume executable-page baseline")?;
                snapshot.ensure_usable()?;
                main_image = Some(layout);
                baseline = Some(snapshot);
            }
            if let Some(gate) = pre_resume_gate {
                wait_for_pre_resume_gate(process.raw(), process_id, &client, layout, gate)?;
            }
            if let Some(gate) = post_unpack_gate {
                post_unpack_arm = Some(arm_post_unpack_hardware_breakpoint(
                    primary_thread.raw(),
                    primary_thread_id,
                    layout,
                    gate.config.target_rva,
                )?);
            }
        }

        let previous_suspend_count = unsafe { ResumeThread(primary_thread.raw()) };
        ensure!(
            previous_suspend_count != u32::MAX,
            "ResumeThread failed: {}",
            windows::core::Error::from_thread()
        );
        ensure!(
            previous_suspend_count > 0,
            "primary thread was unexpectedly not suspended"
        );
        resumed_at = Some(Instant::now());

        if let (Some(gate), Some((arrived_event, release_event)), Some(arm)) = (
            post_unpack_gate,
            post_unpack_events.as_ref(),
            post_unpack_arm.as_ref(),
        ) {
            wait_for_post_unpack_gate(
                process.raw(),
                process_id,
                primary_thread_id,
                &client,
                arm,
                gate,
                arrived_event.raw(),
                release_event.raw(),
            )?;
        }

        wait_for_stage(
            ready_event.raw(),
            process.raw(),
            handshake_timeout,
            "post-unpack hooks READY",
        )
    })();

    if let Err(error) = setup {
        if let Some((_, release_event)) = post_unpack_events.as_ref() {

let _ = unsafe { SetEvent(release_event.raw()) };
        }
        let _ = unsafe { TerminateProcess(process.raw(), 0x474c_5945) };
        let _ = unsafe { WaitForSingleObject(process.raw(), 5_000) };
        return Err(error);
    }

    Ok((
        BootSession {
            process,
            _primary_thread: primary_thread,
            process_id,
            main_image,
        },
        baseline,
        resumed_at,
    ))
}

#[derive(Serialize)]
struct PreResumeGateMetadata {
    schema_version: u32,
    state: &'static str,
    pid: u32,
    image_base: u64,
    image_base_hex: String,
    image_size: u64,
    image_size_hex: String,
    client: String,
    release_file: String,
}

fn wait_for_pre_resume_gate(
    process: HANDLE,
    process_id: u32,
    client: &Path,
    layout: RemoteImageLayout,
    gate: &GateSpec,
) -> Result<()> {
    let metadata = PreResumeGateMetadata {
        schema_version: 1,
        state: "waiting-before-primary-resume",
        pid: process_id,
        image_base: layout.base as u64,
        image_base_hex: format!("0x{:08X}", layout.base),
        image_size: layout.size as u64,
        image_size_hex: format!("0x{:08X}", layout.size),
        client: client.display().to_string(),
        release_file: gate.release_path.display().to_string(),
    };
    let mut serialized = serde_json::to_vec_pretty(&metadata)
        .context("failed to serialize pre-resume gate metadata")?;
    serialized.push(b'\n');
    let mut temporary_metadata_name = OsString::from(gate.metadata_path.as_os_str());
    temporary_metadata_name.push(format!(".tmp-{process_id}"));
    let temporary_metadata_path = PathBuf::from(temporary_metadata_name);
    let mut metadata_file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_metadata_path)
        .with_context(|| {
            format!(
                "failed to create temporary pre-resume gate metadata {}",
                temporary_metadata_path.display()
            )
        })?;
    metadata_file.write_all(&serialized).with_context(|| {
        format!(
            "failed to write pre-resume gate metadata {}",
            temporary_metadata_path.display()
        )
    })?;
    metadata_file.sync_all().with_context(|| {
        format!(
            "failed to flush pre-resume gate metadata {}",
            temporary_metadata_path.display()
        )
    })?;
    drop(metadata_file);
    if let Err(error) = fs::rename(&temporary_metadata_path, &gate.metadata_path) {
        let _ = fs::remove_file(&temporary_metadata_path);
        return Err(error).with_context(|| {
            format!(
                "failed to publish pre-resume gate metadata {}",
                gate.metadata_path.display()
            )
        });
    }

    println!("pre_resume_state=waiting");
    println!("child_pid={process_id}");
    println!("image_base=0x{:08X}", layout.base);
    println!("image_size=0x{:08X}", layout.size);
    println!("pre_resume_gate={}", gate.release_path.display());
    println!("pre_resume_metadata={}", gate.metadata_path.display());
    io::stdout()
        .flush()
        .context("failed to flush pre-resume gate details to stdout")?;

    let started = Instant::now();
    loop {
        match fs::metadata(&gate.release_path) {
            Ok(file) => {
                ensure!(
                    file.is_file(),
                    "pre-resume gate release path is not a regular file: {}",
                    gate.release_path.display()
                );
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect pre-resume gate release file {}",
                        gate.release_path.display()
                    )
                });
            }
        }

        let process_wait = unsafe { WaitForSingleObject(process, 0) };
        if process_wait == WAIT_OBJECT_0 {
            let mut exit_code = 0;
            unsafe { GetExitCodeProcess(process, &mut exit_code) }
                .context("GetExitCodeProcess failed during pre-resume gate")?;
            bail!(
                "client exited with code 0x{exit_code:08x} while waiting for pre-resume gate {}",
                gate.release_path.display()
            );
        }
        ensure!(
            process_wait == WAIT_TIMEOUT,
            "unexpected process wait status 0x{:08x} during pre-resume gate",
            process_wait.0
        );

        let elapsed = started.elapsed();
        ensure!(
            elapsed < gate.timeout,
            "timed out after {} seconds waiting for pre-resume gate {}; primary thread was not resumed",
            gate.timeout.as_secs(),
            gate.release_path.display()
        );
        std::thread::sleep(Duration::from_millis(25).min(gate.timeout.saturating_sub(elapsed)));
    }

    println!("pre_resume_state=released");
    io::stdout()
        .flush()
        .context("failed to flush pre-resume release state to stdout")?;
    Ok(())
}

#[cfg(target_arch = "x86")]
const DR0_ENABLE_MASK: u32 = 0b11;
#[cfg(target_arch = "x86")]
const DR0_CONTROL_MASK: u32 = 0b1111 << 16;
#[cfg(target_arch = "x86")]
const DR6_BREAKPOINT_ZERO: u32 = 1;

#[derive(Clone, Debug)]
struct HardwareBreakpointArm {
    target_rva: u32,
    target_va: u32,
    initial_eip: u32,
    original_dr0: u32,
    original_dr1: u32,
    original_dr2: u32,
    original_dr3: u32,
    original_dr6: u32,
    original_dr7: u32,
    armed_dr0: u32,
    armed_dr6: u32,
    armed_dr7: u32,
    image: RemoteImageLayout,
}

#[cfg(target_arch = "x86")]
fn arm_post_unpack_hardware_breakpoint(
    primary_thread: HANDLE,
    primary_thread_id: u32,
    layout: RemoteImageLayout,
    target_rva: u32,
) -> Result<HardwareBreakpointArm> {
    let target_end = (target_rva as usize)
        .checked_add(1)
        .context("post-unpack target RVA overflow")?;
    ensure!(
        target_end <= layout.size,
        "post-unpack target RVA 0x{target_rva:x} is outside image size 0x{:x}",
        layout.size
    );
    let target_va = layout
        .base
        .checked_add(target_rva as usize)
        .and_then(|address| u32::try_from(address).ok())
        .context("post-unpack target VA does not fit the x86 address space")?;

    let mut context = CONTEXT {
        ContextFlags: CONTEXT_CONTROL_X86 | CONTEXT_DEBUG_REGISTERS_X86,
        ..Default::default()
    };

unsafe { GetThreadContext(primary_thread, &mut context) }.with_context(|| {
        format!("GetThreadContext failed for suspended primary thread {primary_thread_id}")
    })?;
    ensure!(
        context.Dr0 == 0,
        "primary thread {primary_thread_id} DR0 contains 0x{:08x}; refusing to overwrite an existing debug address",
        context.Dr0
    );
    ensure!(
        context.Dr7 & (DR0_ENABLE_MASK | DR0_CONTROL_MASK) == 0,
        "primary thread {primary_thread_id} DR0 is already enabled/configured in DR7=0x{:08x}",
        context.Dr7
    );

    let original = context;
    context.Dr0 = target_va;
    context.Dr6 &= !DR6_BREAKPOINT_ZERO;
    context.Dr7 = (context.Dr7 & !DR0_CONTROL_MASK) | 1;

unsafe { SetThreadContext(primary_thread, &context) }.with_context(|| {
        format!("SetThreadContext failed for suspended primary thread {primary_thread_id}")
    })?;

    let mut verified = CONTEXT {
        ContextFlags: CONTEXT_CONTROL_X86 | CONTEXT_DEBUG_REGISTERS_X86,
        ..Default::default()
    };
    
    unsafe { GetThreadContext(primary_thread, &mut verified) }.with_context(|| {
        format!("GetThreadContext verification failed for primary thread {primary_thread_id}")
    })?;
    ensure!(
        verified.Dr0 == target_va
            && verified.Dr6 & DR6_BREAKPOINT_ZERO == 0
            && verified.Dr7 & DR0_ENABLE_MASK == 1
            && verified.Dr7 & DR0_CONTROL_MASK == 0,
        "hardware execute breakpoint verification failed for primary thread {primary_thread_id}: DR0=0x{:08x}, DR6=0x{:08x}, DR7=0x{:08x}",
        verified.Dr0,
        verified.Dr6,
        verified.Dr7
    );

    Ok(HardwareBreakpointArm {
        target_rva,
        target_va,
        initial_eip: original.Eip,
        original_dr0: original.Dr0,
        original_dr1: original.Dr1,
        original_dr2: original.Dr2,
        original_dr3: original.Dr3,
        original_dr6: original.Dr6,
        original_dr7: original.Dr7,
        armed_dr0: verified.Dr0,
        armed_dr6: verified.Dr6,
        armed_dr7: verified.Dr7,
        image: layout,
    })
}

#[cfg(not(target_arch = "x86"))]
fn arm_post_unpack_hardware_breakpoint(
    _primary_thread: HANDLE,
    _primary_thread_id: u32,
    _layout: RemoteImageLayout,
    _target_rva: u32,
) -> Result<HardwareBreakpointArm> {
    bail!("post-unpack hardware gate requires an i686-pc-windows-msvc launcher")
}

#[derive(Serialize)]
struct PostUnpackGateMetadata {
    schema_version: u32,
    gate_kind: &'static str,
    state: &'static str,
    mechanism: &'static str,
    pid: u32,
    primary_thread_id: u32,
    image_base: u64,
    image_base_hex: String,
    image_size: u64,
    image_size_hex: String,
    target_rva: u64,
    target_rva_hex: String,
    target_va: u64,
    target_va_hex: String,
    hardware_slot: u32,
    breakpoint_type: &'static str,
    breakpoint_length: u32,
    expected_exception_code: u64,
    expected_exception_code_hex: &'static str,
    validated_exception_address: String,
    validated_eip: String,
    validated_dr6_b0: bool,
    validated_dr0: String,
    exception_dispatch_completed_before_wait: bool,
    initial_eip: String,
    original_dr0: String,
    original_dr1: String,
    original_dr2: String,
    original_dr3: String,
    original_dr6: String,
    original_dr7: String,
    armed_dr0: String,
    armed_dr6: String,
    armed_dr7: String,
    arrived_event: String,
    release_event: String,
    release_file: String,
    launcher_stage_timeout_ms: u64,
    shim_fail_open_timeout_ms: u64,
    client: String,
}

#[allow(clippy::too_many_arguments)]
fn wait_for_post_unpack_gate(
    process: HANDLE,
    process_id: u32,
    primary_thread_id: u32,
    client: &Path,
    arm: &HardwareBreakpointArm,
    gate: &PostUnpackGateSpec,
    arrived_event: HANDLE,
    release_event: HANDLE,
) -> Result<()> {
    ensure_release_file_absent(gate, "before the measured RVA was reached")?;
    wait_for_stage(
        arrived_event,
        process,
        gate.timeout,
        "post-unpack hardware breakpoint ARRIVED",
    )?;

ensure_release_file_absent(gate, "before post-unpack metadata publication")?;

    let metadata = PostUnpackGateMetadata {
        schema_version: 1,
        gate_kind: "post-unpack-debugger",
        state: "waiting-after-validated-hardware-breakpoint",
        mechanism: "x86-dr0-execute-veh-register-preserving-thunk",
        pid: process_id,
        primary_thread_id,
        image_base: arm.image.base as u64,
        image_base_hex: format!("0x{:08X}", arm.image.base),
        image_size: arm.image.size as u64,
        image_size_hex: format!("0x{:08X}", arm.image.size),
        target_rva: arm.target_rva as u64,
        target_rva_hex: format!("0x{:08X}", arm.target_rva),
        target_va: arm.target_va as u64,
        target_va_hex: format!("0x{:08X}", arm.target_va),
        hardware_slot: 0,
        breakpoint_type: "execute",
        breakpoint_length: 1,
        expected_exception_code: 0x8000_0004,
        expected_exception_code_hex: "0x80000004",
        validated_exception_address: format!("0x{:08X}", arm.target_va),
        validated_eip: format!("0x{:08X}", arm.target_va),
        validated_dr6_b0: true,
        validated_dr0: format!("0x{:08X}", arm.target_va),
        exception_dispatch_completed_before_wait: true,
        initial_eip: format!("0x{:08X}", arm.initial_eip),
        original_dr0: format!("0x{:08X}", arm.original_dr0),
        original_dr1: format!("0x{:08X}", arm.original_dr1),
        original_dr2: format!("0x{:08X}", arm.original_dr2),
        original_dr3: format!("0x{:08X}", arm.original_dr3),
        original_dr6: format!("0x{:08X}", arm.original_dr6),
        original_dr7: format!("0x{:08X}", arm.original_dr7),
        armed_dr0: format!("0x{:08X}", arm.armed_dr0),
        armed_dr6: format!("0x{:08X}", arm.armed_dr6),
        armed_dr7: format!("0x{:08X}", arm.armed_dr7),
        arrived_event: gate.config.arrived_event.clone(),
        release_event: gate.config.release_event.clone(),
        release_file: gate.release_path.display().to_string(),
        launcher_stage_timeout_ms: u64::try_from(gate.timeout.as_millis()).unwrap_or(u64::MAX),
        shim_fail_open_timeout_ms: gate.config.timeout_ms,
        client: client.display().to_string(),
    };
    publish_gate_metadata(&gate.metadata_path, process_id, &metadata, "post-unpack")?;

    println!("post_unpack_state=waiting");
    println!("child_pid={process_id}");
    println!("primary_thread_id={primary_thread_id}");
    println!("image_base=0x{:08X}", arm.image.base);
    println!("image_size=0x{:08X}", arm.image.size);
    println!("post_unpack_target_rva=0x{:08X}", arm.target_rva);
    println!("post_unpack_target_va=0x{:08X}", arm.target_va);
    println!("post_unpack_gate={}", gate.release_path.display());
    println!("post_unpack_metadata={}", gate.metadata_path.display());
    io::stdout()
        .flush()
        .context("failed to flush post-unpack gate details to stdout")?;

    wait_for_release_file(process, gate, "post-unpack")?;

unsafe { SetEvent(release_event) }.context("failed to signal post-unpack release event")?;
    println!("post_unpack_state=released");
    io::stdout()
        .flush()
        .context("failed to flush post-unpack release state to stdout")?;
    Ok(())
}

fn ensure_release_file_absent(gate: &PostUnpackGateSpec, stage: &str) -> Result<()> {
    match fs::symlink_metadata(&gate.release_path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => bail!(
            "post-unpack release path appeared {stage}: {}",
            gate.release_path.display()
        ),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to inspect post-unpack release path {}",
                gate.release_path.display()
            )
        }),
    }
}

fn wait_for_release_file(process: HANDLE, gate: &PostUnpackGateSpec, label: &str) -> Result<()> {
    let started = Instant::now();
    loop {
        match fs::metadata(&gate.release_path) {
            Ok(file) => {
                ensure!(
                    file.is_file(),
                    "{label} gate release path is not a regular file: {}",
                    gate.release_path.display()
                );
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect {label} gate release file {}",
                        gate.release_path.display()
                    )
                });
            }
        }
        let status = unsafe { WaitForSingleObject(process, 0) };
        if status == WAIT_OBJECT_0 {
            let mut exit_code = 0;
            unsafe { GetExitCodeProcess(process, &mut exit_code) }
                .with_context(|| format!("GetExitCodeProcess failed during {label} gate"))?;
            bail!(
                "client exited with code 0x{exit_code:08x} while waiting for {label} gate {}",
                gate.release_path.display()
            );
        }
        ensure!(
            status == WAIT_TIMEOUT,
            "unexpected process wait status 0x{:08x} during {label} gate",
            status.0
        );
        let elapsed = started.elapsed();
        ensure!(
            elapsed < gate.timeout,
            "timed out after {} seconds waiting for {label} gate {}",
            gate.timeout.as_secs(),
            gate.release_path.display()
        );
        std::thread::sleep(Duration::from_millis(25).min(gate.timeout.saturating_sub(elapsed)));
    }
}

fn publish_gate_metadata(
    path: &Path,
    process_id: u32,
    metadata: &impl Serialize,
    label: &str,
) -> Result<()> {
    let mut serialized = serde_json::to_vec_pretty(metadata)
        .with_context(|| format!("failed to serialize {label} gate metadata"))?;
    serialized.push(b'\n');
    let mut temporary_name = OsString::from(path.as_os_str());
    temporary_name.push(format!(".tmp-{process_id}"));
    let temporary_path = PathBuf::from(temporary_name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .with_context(|| {
            format!(
                "failed to create temporary {label} gate metadata {}",
                temporary_path.display()
            )
        })?;
    file.write_all(&serialized).with_context(|| {
        format!(
            "failed to write {label} gate metadata {}",
            temporary_path.display()
        )
    })?;
    file.sync_all().with_context(|| {
        format!(
            "failed to flush {label} gate metadata {}",
            temporary_path.display()
        )
    })?;
    drop(file);
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error).with_context(|| {
            format!("failed to publish {label} gate metadata {}", path.display())
        });
    }
    Ok(())
}

fn locate_main_image(process: HANDLE, process_id: u32, client: &Path) -> Result<RemoteImageLayout> {
    let snapshot = (0..8)
        .find_map(|_| {

match unsafe {
                CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, process_id)
            } {
                Ok(handle) => Some(Ok(OwnedHandle(handle))),
                Err(error) if win32_error_code(&error) == ERROR_BAD_LENGTH.0 => {
                    std::thread::yield_now();
                    None
                }
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()
        .context("CreateToolhelp32Snapshot(main module) failed")?
        .context("module snapshot repeatedly returned ERROR_BAD_LENGTH")?;

    let mut entry = MODULEENTRY32W {
        dwSize: u32::try_from(size_of::<MODULEENTRY32W>()).expect("MODULEENTRY32W fits u32"),
        ..Default::default()
    };

unsafe { Module32FirstW(snapshot.raw(), &mut entry) }
        .context("Module32FirstW(main module) failed")?;

    let mut observed = Vec::new();
    loop {
        let module_path = path_from_wide_nul(&entry.szExePath);
        observed.push(module_path.display().to_string());
        if paths_equal_windows(&module_path, client) {
            let base = entry.modBaseAddr as usize;
            ensure!(base != 0, "main module snapshot reported a null base");
            let size = read_remote_image_size(process, base)?;
            ensure!(
                size <= MAX_MAIN_IMAGE_SIZE,
                "main image SizeOfImage 0x{size:x} exceeds the 512 MiB capture ceiling"
            );
            ensure!(
                entry.modBaseSize as usize == size,
                "Toolhelp module size 0x{:x} disagrees with PE SizeOfImage 0x{size:x}",
                entry.modBaseSize
            );
            base.checked_add(size)
                .context("main image address extent overflow")?;
            return Ok(RemoteImageLayout { base, size });
        }

match unsafe { Module32NextW(snapshot.raw(), &mut entry) } {
            Ok(()) => {}
            Err(error) if win32_error_code(&error) == ERROR_NO_MORE_FILES.0 => break,
            Err(error) => return Err(error).context("Module32NextW failed"),
        }
    }

    bail!(
        "could not identify client main module {}; observed modules: {}",
        client.display(),
        observed.join(", ")
    )
}

fn read_remote_image_size(process: HANDLE, base: usize) -> Result<usize> {
    let mut dos = [0_u8; 64];
    read_remote_exact(process, base, &mut dos)?;
    ensure!(&dos[..2] == b"MZ", "remote main module has no MZ signature");
    let nt_offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().expect("fixed slice")) as usize;
    ensure!(
        (64..=1024 * 1024).contains(&nt_offset),
        "remote main module has implausible e_lfanew 0x{nt_offset:x}"
    );
    let nt_address = base
        .checked_add(nt_offset)
        .context("remote PE-header address overflow")?;
    let mut nt = [0_u8; 96];
    read_remote_exact(process, nt_address, &mut nt)?;
    ensure!(
        &nt[..4] == b"PE\0\0",
        "remote main module has no PE signature"
    );
    ensure!(
        u16::from_le_bytes(nt[24..26].try_into().expect("fixed slice")) == 0x010b,
        "remote main module is not PE32"
    );
    let size = u32::from_le_bytes(nt[24 + 56..24 + 60].try_into().expect("fixed slice"));
    ensure!(size != 0, "remote PE SizeOfImage is zero");
    Ok(size as usize)
}

fn capture_code_snapshot(process: HANDLE, layout: RemoteImageLayout) -> Result<CodeSnapshot> {
    let mut ranges = Vec::new();
    walk_image_regions(process, layout, |region| {
        if region.executable {
            let bytes = if region.readable {
                read_remote_vec(process, region.address, region.length)?
            } else {
                Vec::new()
            };
            ranges.push(ExecutableRange {
                rva: u32::try_from(region.address - layout.base)
                    .context("executable range RVA does not fit u32")?,
                length: u32::try_from(region.length)
                    .context("executable range length does not fit u32")?,
                protection: region.protection,
                bytes,
            });
        }
        Ok(())
    })?;
    let snapshot = CodeSnapshot { ranges };
    snapshot.ensure_usable()?;
    Ok(snapshot)
}

fn capture_mapped_image(process: HANDLE, layout: RemoteImageLayout) -> Result<MappedImage> {
    let mut image = vec![0_u8; layout.size];
    let mut ranges = Vec::new();
    let mut memory_ranges = Vec::new();
    let mut readable_bytes = 0_usize;
    walk_image_regions(process, layout, |region| {
        let bytes = if region.readable {
            let bytes = read_remote_vec(process, region.address, region.length)?;
            let destination = region.address - layout.base;
            image[destination..destination + region.length].copy_from_slice(&bytes);
            readable_bytes = readable_bytes.saturating_add(region.length);
            bytes
        } else {
            Vec::new()
        };
        if region.executable {
            ranges.push(ExecutableRange {
                rva: u32::try_from(region.address - layout.base)
                    .context("executable range RVA does not fit u32")?,
                length: u32::try_from(region.length)
                    .context("executable range length does not fit u32")?,
                protection: region.protection,
                bytes,
            });
        }
        memory_ranges.push(MemoryRange {
            rva: u32::try_from(region.address - layout.base)
                .context("memory range RVA does not fit u32")?,
            length: u32::try_from(region.length).context("memory range length does not fit u32")?,
            committed: region.committed,
            readable: region.readable,
            writable: region.writable,
            executable: region.executable,
        });
        Ok(())
    })?;
    let code = CodeSnapshot { ranges };
    code.ensure_usable()?;
    Ok(MappedImage {
        base: layout.base,
        bytes: image,
        code,
        memory_ranges,
        readable_bytes,
        zero_filled_bytes: layout.size.saturating_sub(readable_bytes),
    })
}

#[derive(Clone, Copy, Debug)]
struct RemoteRegion {
    address: usize,
    length: usize,
    protection: u32,
    committed: bool,
    readable: bool,
    writable: bool,
    executable: bool,
}

fn walk_image_regions(
    process: HANDLE,
    layout: RemoteImageLayout,
    mut visit: impl FnMut(RemoteRegion) -> Result<()>,
) -> Result<()> {
    let image_end = layout
        .base
        .checked_add(layout.size)
        .context("main image address extent overflow")?;
    let mut cursor = layout.base;
    while cursor < image_end {
        let mut information = MEMORY_BASIC_INFORMATION::default();

let queried = unsafe {
            VirtualQueryEx(
                process,
                Some(cursor as *const c_void),
                &mut information,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            queried == size_of::<MEMORY_BASIC_INFORMATION>(),
            "VirtualQueryEx failed at remote address 0x{cursor:x}: {}",
            windows::core::Error::from_thread()
        );
        let region_base = information.BaseAddress as usize;
        let raw_end = region_base
            .checked_add(information.RegionSize)
            .context("VirtualQueryEx region extent overflow")?;
        ensure!(
            raw_end > cursor,
            "VirtualQueryEx made no progress at 0x{cursor:x}"
        );
        let start = cursor.max(region_base);
        let end = raw_end.min(image_end);
        let protection = information.Protect;
        let committed = information.State == MEM_COMMIT;
        let guarded = protection.contains(PAGE_GUARD);
        let inaccessible = protection.contains(PAGE_NOACCESS);
        let executable = committed
            && (protection.contains(PAGE_EXECUTE)
                || protection.contains(PAGE_EXECUTE_READ)
                || protection.contains(PAGE_EXECUTE_READWRITE)
                || protection.contains(PAGE_EXECUTE_WRITECOPY));
        let writable = committed
            && (protection.contains(PAGE_READWRITE)
                || protection.contains(PAGE_WRITECOPY)
                || protection.contains(PAGE_EXECUTE_READWRITE)
                || protection.contains(PAGE_EXECUTE_WRITECOPY));
        visit(RemoteRegion {
            address: start,
            length: end - start,
            protection: protection.0,
            committed,
            readable: committed && !guarded && !inaccessible,
            writable,
            executable,
        })?;
        cursor = end;
    }
    Ok(())
}

fn read_remote_vec(process: HANDLE, address: usize, length: usize) -> Result<Vec<u8>> {
    let mut bytes = vec![0_u8; length];
    for (chunk_index, destination) in bytes.chunks_mut(REMOTE_READ_CHUNK).enumerate() {
        let offset = chunk_index
            .checked_mul(REMOTE_READ_CHUNK)
            .context("remote read offset overflow")?;
        read_remote_exact(
            process,
            address
                .checked_add(offset)
                .context("remote read address overflow")?,
            destination,
        )?;
    }
    Ok(bytes)
}

fn read_remote_exact(process: HANDLE, address: usize, destination: &mut [u8]) -> Result<()> {
    let mut read = 0_usize;

unsafe {
        ReadProcessMemory(
            process,
            address as *const c_void,
            destination.as_mut_ptr().cast::<c_void>(),
            destination.len(),
            Some(&mut read),
        )
    }
    .with_context(|| format!("ReadProcessMemory failed at 0x{address:x}"))?;
    ensure!(
        read == destination.len(),
        "ReadProcessMemory returned {read} of {} bytes at 0x{address:x}",
        destination.len()
    );
    Ok(())
}

fn path_from_wide_nul(buffer: &[u16]) -> PathBuf {
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    PathBuf::from(OsString::from_wide(&buffer[..length]))
}

fn paths_equal_windows(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left.as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
}

fn win32_error_code(error: &windows::core::Error) -> u32 {
    (error.code().0 as u32) & 0xffff
}

fn create_named_event(name: &str) -> Result<OwnedHandle> {
    let wide = wide_nul(OsStr::new(name))?;
    let handle = unsafe { CreateEventW(None, true, false, PCWSTR(wide.as_ptr())) }?;

if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let _ = unsafe { CloseHandle(handle) };
        bail!("named event already exists: {name}");
    }
    Ok(OwnedHandle(handle))
}

fn create_suspended_process(
    client: &Path,
    region: &str,
    runparam_key: Option<&str>,
    environment: &[u16],
) -> Result<(OwnedHandle, OwnedHandle, u32, u32)> {
    let current_directory = client
        .parent()
        .context("client path has no parent directory")?;
    let client_wide = wide_nul(client.as_os_str())?;
    let directory_wide = wide_nul(current_directory.as_os_str())?;
    let mut command_line = client_command_line(client.as_os_str(), region, runparam_key)?;
    let startup = STARTUPINFOW {
        cb: u32::try_from(size_of::<STARTUPINFOW>()).expect("STARTUPINFOW fits u32"),
        ..Default::default()
    };
    let mut information = PROCESS_INFORMATION::default();
    let flags = CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT;

    let created = unsafe {
        CreateProcessW(
            PCWSTR(client_wide.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            flags,
            Some(environment.as_ptr().cast::<c_void>()),
            PCWSTR(directory_wide.as_ptr()),
            &startup,
            &mut information,
        )
    };
    if let Err(error) = created {
        let raw_code = (error.code().0 as u32) & 0xffff;
        if raw_code == ERROR_ELEVATION_REQUIRED.0 {
            bail!(
                "Windows returned ERROR_ELEVATION_REQUIRED (740) for {}; launch the manifest-enabled i686 goley-boot executable and approve its UAC prompt",
                client.display()
            );
        }
        return Err(anyhow!(error))
            .with_context(|| format!("CreateProcessW failed for {}", client.display()));
    }

    Ok((
        OwnedHandle(information.hProcess),
        OwnedHandle(information.hThread),
        information.dwProcessId,
        information.dwThreadId,
    ))
}

fn inject_load_library(process: HANDLE, shim: &Path) -> Result<()> {
    let shim_wide = wide_nul(shim.as_os_str())?;
    let byte_len = shim_wide
        .len()
        .checked_mul(size_of::<u16>())
        .context("shim path length overflow")?;
    let remote = unsafe {
        VirtualAllocEx(
            process,
            None,
            byte_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    ensure!(
        !remote.is_null(),
        "VirtualAllocEx failed: {}",
        windows::core::Error::from_thread()
    );
    let allocation = RemoteAllocation {
        process,
        address: remote,
    };

    let mut bytes_written = 0_usize;
    unsafe {
        WriteProcessMemory(
            process,
            remote,
            shim_wide.as_ptr().cast::<c_void>(),
            byte_len,
            Some(&mut bytes_written),
        )
    }
    .context("WriteProcessMemory failed while copying shim path")?;
    ensure!(
        bytes_written == byte_len,
        "WriteProcessMemory copied {bytes_written} of {byte_len} bytes"
    );

    let kernel32_name = wide_nul(OsStr::new("kernel32.dll"))?;
    let kernel32 = unsafe { GetModuleHandleW(PCWSTR(kernel32_name.as_ptr())) }
        .context("GetModuleHandleW(kernel32.dll) failed")?;
    let load_library =
        unsafe { GetProcAddress(kernel32, PCSTR(c"LoadLibraryW".as_ptr().cast::<u8>())) }
            .context("GetProcAddress(LoadLibraryW) returned null")?;
    let start_routine: LPTHREAD_START_ROUTINE = Some(unsafe {
        std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            unsafe extern "system" fn(*mut c_void) -> u32,
        >(load_library)
    });

    let remote_thread = unsafe {
        CreateRemoteThread(
            process,
            None,
            0,
            start_routine,
            Some(remote.cast_const()),
            0,
            None,
        )
    }
    .context("CreateRemoteThread(LoadLibraryW) failed")?;
    let remote_thread = OwnedHandle(remote_thread);
    let wait =
        unsafe { WaitForSingleObject(remote_thread.raw(), duration_ms(LOAD_LIBRARY_TIMEOUT)) };
    ensure!(wait != WAIT_TIMEOUT, "remote LoadLibraryW call timed out");
    ensure!(
        wait == WAIT_OBJECT_0,
        "remote LoadLibraryW wait failed with status 0x{:08x}",
        wait.0
    );

    let mut module_handle = 0_u32;
    unsafe { GetExitCodeThread(remote_thread.raw(), &mut module_handle) }
        .context("GetExitCodeThread(LoadLibraryW) failed")?;
    ensure!(
        module_handle != 0,
        "remote LoadLibraryW returned NULL for {}",
        shim.display()
    );
    drop(allocation);
    Ok(())
}

fn wait_for_stage(event: HANDLE, process: HANDLE, timeout: Duration, stage: &str) -> Result<()> {
    let status = unsafe { WaitForMultipleObjects(&[event, process], false, duration_ms(timeout)) };
    if status == WAIT_OBJECT_0 {
        return Ok(());
    }
    if status.0 == WAIT_OBJECT_0.0 + 1 {
        let mut exit_code = 0;
        unsafe { GetExitCodeProcess(process, &mut exit_code) }
            .context("GetExitCodeProcess failed during handshake")?;
        bail!("client exited with code 0x{exit_code:08x} before {stage}");
    }
    if status == WAIT_TIMEOUT {
        bail!("timed out waiting for {stage}");
    }
    if status == WAIT_FAILED {
        return Err(windows::core::Error::from_thread())
            .with_context(|| format!("WaitForMultipleObjects failed during {stage}"));
    }
    bail!("unexpected wait status 0x{:08x} during {stage}", status.0)
}

fn unicode_environment(config: &ShimConfig) -> Result<Vec<u16>> {
    let mut variables: BTreeMap<String, (OsString, OsString)> = BTreeMap::new();
    for (key, value) in std::env::vars_os() {

if key.to_string_lossy().starts_with('=') {
            continue;
        }
        let sort_key = key.to_string_lossy().to_uppercase();
        variables.insert(sort_key, (key, value));
    }
    let config_value = config.to_environment_value()?;
    variables.insert(
        SHIM_CONFIG_ENV.to_owned(),
        (OsString::from(SHIM_CONFIG_ENV), config_value),
    );

    let mut block = Vec::new();
    for (_, (key, value)) in variables {
        append_environment_component(&mut block, &key, true)?;
        block.push(u16::from(b'='));
        append_environment_component(&mut block, &value, false)?;
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn append_environment_component(block: &mut Vec<u16>, value: &OsStr, key: bool) -> Result<()> {
    let encoded = value.encode_wide().collect::<Vec<_>>();
    ensure!(
        !encoded.contains(&0),
        "child environment contains an embedded NUL"
    );
    if key {
        ensure!(
            !encoded.contains(&u16::from(b'=')),
            "child environment key contains '='"
        );
    }
    block.extend(encoded);
    Ok(())
}

fn client_command_line(path: &OsStr, region: &str, runparam_key: Option<&str>) -> Result<Vec<u16>> {
    if let Some(runparam_key) = runparam_key {
        ensure!(
            !runparam_key.contains(['\'', '"']),
            "client runparam key contains a quote character"
        );
    }
    let mut command = Vec::new();
    push_quoted_argument(&mut command, path)?;
    command.push(u16::from(b' '));
    push_unquoted_token(&mut command, OsStr::new(region))?;
    command.push(u16::from(b' '));
    push_unquoted_token(&mut command, OsStr::new(runparam_key.unwrap_or("NoPopup")))?;
    command.push(0);
    Ok(command)
}

fn push_unquoted_token(command: &mut Vec<u16>, argument: &OsStr) -> Result<()> {
    let encoded = argument.encode_wide().collect::<Vec<_>>();
    ensure!(!encoded.is_empty(), "client command-line token is empty");
    ensure!(
        !encoded.iter().any(|unit| {
            *unit == 0
                || *unit == u16::from(b'\"')
                || char::from_u32(u32::from(*unit)).is_some_and(char::is_whitespace)
        }),
        "client command-line token requires quoting, which the fixed client does not parse"
    );
    command.extend(encoded);
    Ok(())
}

fn push_quoted_argument(command: &mut Vec<u16>, argument: &OsStr) -> Result<()> {
    let encoded = argument.encode_wide().collect::<Vec<_>>();
    ensure!(
        !encoded.contains(&0),
        "client command-line argument contains an embedded NUL"
    );
    let quote = u16::from(b'"');
    command.push(quote);
    let mut backslashes = 0_usize;
    for unit in encoded {
        if unit == u16::from(b'\\') {
            backslashes += 1;
            continue;
        }
        if unit == quote {
            command.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2 + 1));
        } else {
            command.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
        }
        backslashes = 0;
        command.push(unit);
    }
    
    command.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    command.push(quote);
    Ok(())
}

fn wide_nul(value: &OsStr) -> Result<Vec<u16>> {
    let mut encoded = value.encode_wide().collect::<Vec<_>>();
    ensure!(
        !encoded.contains(&0),
        "wide string contains an embedded NUL"
    );
    encoded.push(0);
    Ok(encoded)
}

fn duration_ms(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX - 1)
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    const fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

struct RemoteAllocation {
    process: HANDLE,
    address: *mut c_void,
}

impl Drop for RemoteAllocation {
    fn drop(&mut self) {
        let _ = unsafe { VirtualFreeEx(self.process, self.address, 0, MEM_RELEASE) };
    }
}

#[cfg(test)]
mod tests {
    use super::client_command_line;
    use std::ffi::OsStr;

    #[test]
    fn client_command_line_preserves_launcher_contract() {
        let encoded = client_command_line(
            OsStr::new(r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin"),
            "TRAuth",
            None,
        )
        .expect("command line should encode");
        let text = String::from_utf16(&encoded[..encoded.len() - 1]).expect("valid UTF-16");
        assert_eq!(
            text,
            r#""C:\Joygame\Goley\BinaryTr\BinaryTr.bin" TRAuth NoPopup"#
        );
    }

    #[test]
    fn client_command_line_rejects_a_region_that_needs_quotes() {
        let error = client_command_line(
            OsStr::new(r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin"),
            "TR Auth",
            None,
        )
        .expect_err("the fixed client consumes raw launcher tokens");
        assert!(error.to_string().contains("does not parse"));
    }

    #[test]
    fn client_command_line_uses_measured_runparam_key() {
        let encoded = client_command_line(
            OsStr::new(r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin"),
            "TRAuth",
            Some("TOKEN"),
        )
        .expect("command line should encode");
        let text = String::from_utf16(&encoded[..encoded.len() - 1]).expect("valid UTF-16");
        assert_eq!(
            text,
            r#""C:\Joygame\Goley\BinaryTr\BinaryTr.bin" TRAuth TOKEN"#
        );
    }

    #[test]
    fn client_command_line_rejects_invalid_runparam_keys() {
        for runparam_key in [
            "",
            "two words",
            "tab\tkey",
            "quote\"key",
            "quote'key",
            "nul\0key",
        ] {
            client_command_line(
                OsStr::new(r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin"),
                "TRAuth",
                Some(runparam_key),
            )
            .expect_err("runparam must remain one unquoted client token");
        }
    }
}
