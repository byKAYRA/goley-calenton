

use std::{cell::Cell, sync::OnceLock};

use retour::GenericDetour;
use thiserror::Error;
use tracing::warn;
use windows::Win32::{
    Foundation::HANDLE,
    System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetProcessId},
};
use windows::core::BOOL;

use crate::platform::{HookError, capture_caller, resolve_export};

type ExitProcessFn = unsafe extern "system" fn(u32);
type TerminateProcessFn = unsafe extern "system" fn(HANDLE, u32) -> BOOL;
type NtTerminateProcessFn = unsafe extern "system" fn(HANDLE, i32) -> i32;
type RtlExitUserProcessFn = unsafe extern "system" fn(i32);

static EXIT_PROCESS: OnceLock<GenericDetour<ExitProcessFn>> = OnceLock::new();
static TERMINATE_PROCESS: OnceLock<GenericDetour<TerminateProcessFn>> = OnceLock::new();
static NT_TERMINATE_PROCESS: OnceLock<GenericDetour<NtTerminateProcessFn>> = OnceLock::new();
static RTL_EXIT_USER_PROCESS: OnceLock<GenericDetour<RtlExitUserProcessFn>> = OnceLock::new();

thread_local! {
    static INSIDE_EXIT_HOOK: Cell<bool> = const { Cell::new(false) };
}

macro_rules! install_detour {
    ($slot:ident, $ty:ty, $module:literal, $symbol:expr, $hook:path) => {{
        let address = resolve_export($module, $symbol)?;
        
        let target: $ty = unsafe { std::mem::transmute(address) };

let detour = unsafe { GenericDetour::<$ty>::new(target, $hook) }.map_err(|error| {
            HookError::Detour {
                symbol: $symbol.to_str().unwrap_or("<invalid>"),
                detail: error.to_string(),
            }
        })?;
        $slot
            .set(detour)
            .map_err(|_| HookError::AlreadyInitialized($symbol.to_str().unwrap_or("<invalid>")))?;
        
        unsafe { $slot.get().expect("hook slot initialized").enable() }.map_err(|error| {
            HookError::Detour {
                symbol: $symbol.to_str().unwrap_or("<invalid>"),
                detail: error.to_string(),
            }
        })?;
        Ok::<(), HookError>(())
    }};
}

pub fn install_hooks() -> Result<(), ExitGuardError> {
    install_detour!(
        EXIT_PROCESS,
        ExitProcessFn,
        "kernel32.dll",
        c"ExitProcess",
        hook_exit_process
    )?;
    install_detour!(
        TERMINATE_PROCESS,
        TerminateProcessFn,
        "kernel32.dll",
        c"TerminateProcess",
        hook_terminate_process
    )?;
    install_detour!(
        NT_TERMINATE_PROCESS,
        NtTerminateProcessFn,
        "ntdll.dll",
        c"NtTerminateProcess",
        hook_nt_terminate_process
    )?;
    install_detour!(
        RTL_EXIT_USER_PROCESS,
        RtlExitUserProcessFn,
        "ntdll.dll",
        c"RtlExitUserProcess",
        hook_rtl_exit_user_process
    )?;
    Ok(())
}

unsafe extern "system" fn hook_exit_process(exit_code: u32) {
    let Some(_scope) = ExitHookScope::enter() else {
        unsafe { EXIT_PROCESS.get().expect("enabled hook").call(exit_code) };
        return;
    };
    log_suppressed("ExitProcess", exit_code as i64);
}

unsafe extern "system" fn hook_terminate_process(process: HANDLE, exit_code: u32) -> BOOL {
    let Some(_scope) = ExitHookScope::enter() else {
        return unsafe {
            TERMINATE_PROCESS
                .get()
                .expect("enabled hook")
                .call(process, exit_code)
        };
    };
    if is_current_process(process) {
        log_suppressed("TerminateProcess", exit_code as i64);
        return BOOL(1);
    }
    unsafe {
        TERMINATE_PROCESS
            .get()
            .expect("enabled hook")
            .call(process, exit_code)
    }
}

unsafe extern "system" fn hook_nt_terminate_process(process: HANDLE, status: i32) -> i32 {
    let Some(_scope) = ExitHookScope::enter() else {
        return unsafe {
            NT_TERMINATE_PROCESS
                .get()
                .expect("enabled hook")
                .call(process, status)
        };
    };
    if is_current_process(process) {
        log_suppressed("NtTerminateProcess", status as i64);
        return 0;
    }
    unsafe {
        NT_TERMINATE_PROCESS
            .get()
            .expect("enabled hook")
            .call(process, status)
    }
}

unsafe extern "system" fn hook_rtl_exit_user_process(status: i32) {
    let Some(_scope) = ExitHookScope::enter() else {
        unsafe {
            RTL_EXIT_USER_PROCESS
                .get()
                .expect("enabled hook")
                .call(status)
        };
        return;
    };
    log_suppressed("RtlExitUserProcess", status as i64);
}

fn is_current_process(process: HANDLE) -> bool {
    if process.0.is_null() || process == unsafe { GetCurrentProcess() } {
        return true;
    }

let target_pid = unsafe { GetProcessId(process) };
    target_pid != 0 && target_pid == unsafe { GetCurrentProcessId() }
}

fn log_suppressed(api: &'static str, status: i64) {
    let caller = capture_caller();
    warn!(
        event_type = "self_termination_suppressed",
        api,
        status,
        caller_module = %caller.module,
        caller_offset = caller.offset as u64,
        caller_address = caller.address as u64,
        "client self-termination request suppressed"
    );
}

struct ExitHookScope;

impl ExitHookScope {
    fn enter() -> Option<Self> {
        INSIDE_EXIT_HOOK.with(|inside| {
            if inside.replace(true) {
                None
            } else {
                Some(Self)
            }
        })
    }
}

impl Drop for ExitHookScope {
    fn drop(&mut self) {
        INSIDE_EXIT_HOOK.with(|inside| inside.set(false));
    }
}

#[derive(Debug, Error)]
pub enum ExitGuardError {
    
    #[error(transparent)]
    Hook(#[from] HookError),
}
