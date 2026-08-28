

use std::sync::atomic::{AtomicUsize, Ordering};

use tracing::info;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, STATUS_SINGLE_STEP},
    System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, CONTEXT, CONTEXT_CONTROL_X86,
        CONTEXT_DEBUG_REGISTERS_X86, EXCEPTION_CONTINUE_EXECUTION, EXCEPTION_CONTINUE_SEARCH,
        EXCEPTION_POINTERS, GetThreadContext, SetThreadContext,
    },
    System::Memory::{PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect},
    System::Threading::{
        GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread, SuspendThread,
        THREAD_GET_CONTEXT, THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME,
    },
};

use crate::platform::HookError;

const GG_EXIT_RVA: u32 = 0x04ba177;

const NOP5: [u8; 5] = [0x90; 5];

#[cfg(target_arch = "x86")]
const DR7_L0: u32 = 1 << 0;
#[cfg(target_arch = "x86")]
const DR7_GE: u32 = 1 << 1;
#[cfg(target_arch = "x86")]
const DR7_LE: u32 = 1 << 2;
#[cfg(target_arch = "x86_64")]
const DR7_L0: u64 = 1 << 0;
#[cfg(target_arch = "x86_64")]
const DR7_GE: u64 = 1 << 1;
#[cfg(target_arch = "x86_64")]
const DR7_LE: u64 = 1 << 2;

static GG_EXIT_ADDR: AtomicUsize = AtomicUsize::new(0);

static MAIN_THREAD_HANDLE: AtomicUsize = AtomicUsize::new(0);

fn find_main_thread_id() -> Option<u32> {
    
    let snapshot = unsafe {
        windows::Win32::System::Diagnostics::ToolHelp::CreateToolhelp32Snapshot(
            windows::Win32::System::Diagnostics::ToolHelp::TH32CS_SNAPTHREAD,
            0,
        )
    }
    .ok()?;

    let our_pid = unsafe { GetCurrentProcessId() };
    let current_tid = unsafe { GetCurrentThreadId() };

    let mut entry = windows::Win32::System::Diagnostics::ToolHelp::THREADENTRY32 {
        dwSize: std::mem::size_of::<
            windows::Win32::System::Diagnostics::ToolHelp::THREADENTRY32,
        >() as u32,
        ..Default::default()
    };

    let mut min_tid = u32::MAX;
    let first = unsafe {
        windows::Win32::System::Diagnostics::ToolHelp::Thread32First(snapshot, &mut entry)
    };
    if first.is_ok() {
        loop {
            if entry.th32OwnerProcessID == our_pid
                && entry.th32ThreadID != current_tid
                && entry.th32ThreadID < min_tid
            {
                min_tid = entry.th32ThreadID;
            }
            if unsafe {
                windows::Win32::System::Diagnostics::ToolHelp::Thread32Next(snapshot, &mut entry)
            }
            .is_err()
            {
                break;
            }
        }
    }

    let _ = unsafe { CloseHandle(snapshot) };
    if min_tid != u32::MAX {
        Some(min_tid)
    } else {
        None
    }
}

fn set_hw_breakpoint(handle: HANDLE, addr: usize) -> Result<(), HookError> {
    unsafe { SuspendThread(handle) };

    let mut context = CONTEXT {
        ContextFlags: CONTEXT_CONTROL_X86 | CONTEXT_DEBUG_REGISTERS_X86,
        ..Default::default()
    };
    unsafe {
        GetThreadContext(handle, &mut context).map_err(|e| HookError::Detour {
            symbol: "GetThreadContext",
            detail: e.to_string(),
        })?;
    }

    context.Dr0 = addr as _;
    context.Dr7 |= DR7_L0 | DR7_GE | DR7_LE;

    unsafe {
        SetThreadContext(handle, &context).map_err(|e| HookError::Detour {
            symbol: "SetThreadContext",
            detail: e.to_string(),
        })?;
        ResumeThread(handle);
    }

    Ok(())
}

fn clear_hw_breakpoint(handle: HANDLE) -> Result<(), HookError> {
    unsafe { SuspendThread(handle) };

    let mut context = CONTEXT {
        ContextFlags: CONTEXT_CONTROL_X86 | CONTEXT_DEBUG_REGISTERS_X86,
        ..Default::default()
    };
    unsafe {
        GetThreadContext(handle, &mut context).map_err(|e| HookError::Detour {
            symbol: "GetThreadContext",
            detail: e.to_string(),
        })?;
    }

    context.Dr0 = 0;
    context.Dr7 &= !(DR7_L0 | DR7_GE | DR7_LE);

    unsafe {
        SetThreadContext(handle, &context).map_err(|e| HookError::Detour {
            symbol: "SetThreadContext",
            detail: e.to_string(),
        })?;
        ResumeThread(handle);
    }

    Ok(())
}

pub fn install(gg_exit_va: usize) -> Result<(), HookError> {
    GG_EXIT_ADDR.store(gg_exit_va, Ordering::Relaxed);

    let main_tid = find_main_thread_id().ok_or_else(|| HookError::Detour {
        symbol: "find_main_thread_id",
        detail: "could not locate main thread".into(),
    })?;

    let handle = unsafe {
        OpenThread(
            THREAD_SET_CONTEXT | THREAD_GET_CONTEXT | THREAD_SUSPEND_RESUME,
            false.into(),
            main_tid,
        )
    }
    .map_err(|e| HookError::Detour {
        symbol: "OpenThread",
        detail: e.to_string(),
    })?;

    set_hw_breakpoint(handle, gg_exit_va)?;

    info!(
        event_type = "gg_bypass_hwbp_set",
        main_tid,
        gg_exit_va = gg_exit_va as u64,
        "hardware execution breakpoint installed on GG exit call-site"
    );

unsafe {
        AddVectoredExceptionHandler(1, Some(gg_exit_veh));
    }

    MAIN_THREAD_HANDLE.store(handle.0 as usize, Ordering::Relaxed);

    Ok(())
}

unsafe extern "system" fn gg_exit_veh(exception_info: *mut EXCEPTION_POINTERS) -> i32 {
    unsafe {
        if exception_info.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let pointers = &*exception_info;
        if pointers.ExceptionRecord.is_null() || pointers.ContextRecord.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let record = &*pointers.ExceptionRecord;

        if record.ExceptionCode != STATUS_SINGLE_STEP {
            return EXCEPTION_CONTINUE_SEARCH;
        }

        let fault_addr = record.ExceptionAddress as usize;
        let target = GG_EXIT_ADDR.load(Ordering::Relaxed);

        if fault_addr != target {
            return EXCEPTION_CONTINUE_SEARCH;
        }

let page_base = (fault_addr & !0xFFF) as *mut core::ffi::c_void;
        let mut old_protect = PAGE_PROTECTION_FLAGS::default();
        VirtualProtect(
            page_base,
            8,
            PAGE_EXECUTE_READWRITE,
            &mut old_protect,
        )
        .ok();

        let write_ptr = fault_addr as *mut u8;
        write_ptr.copy_from_nonoverlapping(NOP5.as_ptr(), 5);

        VirtualProtect(page_base, 8, old_protect, &mut old_protect).ok();

let handle_val = MAIN_THREAD_HANDLE.load(Ordering::Relaxed);
        if handle_val != 0 {
            let h = HANDLE(handle_val as *mut _);
            let _ = clear_hw_breakpoint(h);
            let _ = CloseHandle(h);
        }

windows::Win32::System::Diagnostics::Debug::FlushInstructionCache(
            windows::Win32::System::Threading::GetCurrentProcess(),
            Some(fault_addr as *const _),
            5,
        )
        .ok();

        info!(
            event_type = "gg_bypass_patched",
            address = fault_addr as u64,
            "NOP'd call ExitProcess at GG exit call-site via hardware breakpoint"
        );

        EXCEPTION_CONTINUE_EXECUTION
    }
}

pub fn gg_exit_va() -> usize {
    let module = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }
        .expect("main module handle")
        .0 as usize;
    module + GG_EXIT_RVA as usize
}
