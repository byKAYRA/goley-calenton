

#![allow(missing_docs)]

use std::{convert::Infallible, ffi::c_void, panic, thread};

use thiserror::Error;
use tracing::{error, info, warn};
use windows::Win32::{
    Foundation::{CloseHandle, HINSTANCE, HMODULE},
    System::{
        LibraryLoader::DisableThreadLibraryCalls,
        SystemServices::DLL_PROCESS_ATTACH,
        Threading::{CreateThread, THREAD_CREATION_FLAGS},
    },
};
use windows::core::BOOL;

pub mod config;
mod debugger_gate;

pub mod exitguard;

pub mod gameguard;

mod gg_bypass;
mod gate_patch;
mod handshake;

pub mod logging;

pub mod netredirect;

pub mod patching;
mod platform;

pub mod themida;

pub mod wait_capture;

use config::{ShimConfig, ShimMode};

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    module: HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if reason != DLL_PROCESS_ATTACH {
        return BOOL(1);
    }

    platform::set_shim_module(module.0 as usize);
    
    if unsafe { DisableThreadLibraryCalls(HMODULE(module.0)) }.is_err() {
        return BOOL(0);
    }

match unsafe {
        CreateThread(
            None,
            0,
            Some(worker_entry),
            None,
            THREAD_CREATION_FLAGS(0),
            None,
        )
    } {
        Ok(thread_handle) => {
            let _ = unsafe { CloseHandle(thread_handle) };
            BOOL(1)
        }
        Err(_) => BOOL(0),
    }
}

unsafe extern "system" fn worker_entry(_parameter: *mut c_void) -> u32 {
    let result = panic::catch_unwind(worker_main);
    match result {
        Ok(Ok(never)) => match never {},
        Ok(Err(runtime_error)) => {
            error!(
                event_type = "shim_startup_error",
                error = %runtime_error,
                "shim worker did not reach ready state"
            );
            1
        }
        Err(_) => {
            error!(
                event_type = "shim_worker_panic",
                "shim worker panicked before ready state"
            );
            2
        }
    }
}

fn worker_main() -> Result<Infallible, RuntimeError> {
    let config = ShimConfig::from_env()?;
    let handshake = handshake::Handshake::open(&config)?;
    let logging_guard = logging::init(&config.log_path, &config.verbosity)?;
    debugger_gate::install(&config)?;
    if let Some(gate) = &config.post_unpack_gate {
        info!(
            event_type = "post_unpack_gate_veh_ready",
            target_rva = gate.target_rva as u64,
            arrived_event = %gate.arrived_event,
            release_event = %gate.release_event,
            shim_fail_open_timeout_ms = gate.timeout_ms,
            "post-unpack VEH and register-preserving gate thunk are ready before LOADED"
        );
    }
    let dump_mode = config.mode == ShimMode::DumpUnpacked;

exitguard::install_hooks()?;
    info!(
        event_type = "exitguard_armed",
        mode = ?config.mode,
        "self-termination hooks installed before primary-thread resume"
    );
    if !dump_mode {
        if let Err(e) = gg_bypass::install(gg_bypass::gg_exit_va()) {
            warn!(error = %e, "gg_bypass hardware breakpoint installation failed; fallback to exitguard only");
        }
    }
    handshake.signal_loaded()?;

    info!(
        event_type = "shim_loaded",
        mode = ?config.mode,
        region = ?config.region,
        entry = ?config.entry,
        log_path = %config.log_path.display(),
        "shim worker loaded"
    );

    if dump_mode {

info!(
            event_type = "dump_observer_ready",
            "dump mode installed only self-termination observation hooks"
        );
    } else {
        let readiness = themida::wait_until_ready(&config.unpack)?;
        info!(
            event_type = "unpack_ready",
            probe_rva = readiness.probe_rva as u64,
            measured_rva = readiness.measured_rva,
            elapsed_ms = readiness.elapsed.as_millis() as u64,
            sample = %hex::encode_upper(readiness.sample),
            "Themida/OEP readiness heuristic passed without a software breakpoint"
        );
        if !readiness.measured_rva {
            warn!(
                event_type = "unpack_fallback",
                "no measured OEP RVA was configured; PE entry point was used as a fallback heuristic"
            );
        }

        debugger_gate::promote(&config)?;
        if config.post_unpack_gate.is_some() {
            info!(
                event_type = "post_unpack_gate_veh_promoted",
                "post-unpack VEH was re-registered first after unpack readiness"
            );
        }

        patching::apply_configured(&config)?;
        gameguard::initialize(&config)?;
        wait_capture::initialize()?;
        wait_capture::install_hooks()?;
    }

    let netredirect = netredirect::initialize(config.entry.as_deref())?;
    info!(
        event_type = "netredirect_state",
        state = ?netredirect,
        "network redirection initialization completed"
    );

    handshake.signal_ready()?;
    info!(
        event_type = "shim_ready",
        gameguard_event = ?config.gameguard_ready_event,
        "shim hooks are ready"
    );

if !dump_mode {
        gate_patch::patch_config_gates();
    }

let _logging_guard = logging_guard;
    if config.post_unpack_gate.is_none() {
        loop {
            thread::park();
        }
    }

    let snapshot = debugger_gate::wait_for_snapshot()?;
    info!(
        event_type = "post_unpack_gate_snapshot",
        primary_thread_id = snapshot.primary_thread_id as u64,
        eax = snapshot.eax as u64,
        ebx = snapshot.ebx as u64,
        ecx = snapshot.ecx as u64,
        edx = snapshot.edx as u64,
        esi = snapshot.esi as u64,
        edi = snapshot.edi as u64,
        ebp = snapshot.ebp as u64,
        esp = snapshot.esp as u64,
        eip = snapshot.eip as u64,
        eflags = snapshot.eflags as u64,
        dr6 = snapshot.dr6 as u64,
        dr7 = snapshot.dr7 as u64,
        "captured the original primary-thread context at the exact post-unpack gate hit"
    );
    loop {
        thread::park();
    }
}

#[derive(Debug, Error)]
enum RuntimeError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Handshake(#[from] handshake::HandshakeError),
    #[error(transparent)]
    Logging(#[from] logging::LoggingError),
    #[error(transparent)]
    Themida(#[from] themida::ThemidaError),
    #[error(transparent)]
    Patch(#[from] patching::PatchError),
    #[error(transparent)]
    GameGuard(#[from] gameguard::GameGuardError),
    #[error(transparent)]
    WaitCapture(#[from] wait_capture::WaitCaptureError),
    #[error(transparent)]
    ExitGuard(#[from] exitguard::ExitGuardError),
    #[error(transparent)]
    DebuggerGate(#[from] debugger_gate::DebuggerGateError),
    #[error(transparent)]
    NetRedirect(#[from] netredirect::NetRedirectError),
}
