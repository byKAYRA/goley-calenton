

use thiserror::Error;

use crate::config::ShimConfig;

pub(crate) fn install(config: &ShimConfig) -> Result<(), DebuggerGateError> {
    let Some(gate) = config.post_unpack_gate.as_ref() else {
        return Ok(());
    };
    install_platform(gate)
}

pub(crate) fn promote(config: &ShimConfig) -> Result<(), DebuggerGateError> {
    if config.post_unpack_gate.is_none() {
        return Ok(());
    }
    promote_platform()
}

pub(crate) fn wait_for_snapshot() -> Result<GateSnapshot, DebuggerGateError> {
    wait_for_snapshot_platform()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GateSnapshot {
    pub(crate) primary_thread_id: u32,
    pub(crate) eax: u32,
    pub(crate) ebx: u32,
    pub(crate) ecx: u32,
    pub(crate) edx: u32,
    pub(crate) esi: u32,
    pub(crate) edi: u32,
    pub(crate) ebp: u32,
    pub(crate) esp: u32,
    pub(crate) eip: u32,
    pub(crate) eflags: u32,
    pub(crate) dr6: u32,
    pub(crate) dr7: u32,
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_platform(config: &crate::config::PostUnpackGateConfig) -> Result<(), DebuggerGateError> {
    imp::install(config)
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_platform(
    _config: &crate::config::PostUnpackGateConfig,
) -> Result<(), DebuggerGateError> {
    Err(DebuggerGateError::UnsupportedTarget)
}

#[cfg(all(windows, target_arch = "x86"))]
fn promote_platform() -> Result<(), DebuggerGateError> {
    imp::promote()
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn promote_platform() -> Result<(), DebuggerGateError> {
    Err(DebuggerGateError::UnsupportedTarget)
}

#[cfg(all(windows, target_arch = "x86"))]
fn wait_for_snapshot_platform() -> Result<GateSnapshot, DebuggerGateError> {
    imp::wait_for_snapshot()
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn wait_for_snapshot_platform() -> Result<GateSnapshot, DebuggerGateError> {
    Err(DebuggerGateError::UnsupportedTarget)
}

#[cfg(all(windows, target_arch = "x86"))]
mod imp {
    use std::cell::UnsafeCell;
    use std::ffi::{OsStr, c_void};
    use std::mem::MaybeUninit;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    };

    use windows::Win32::{
        Foundation::{CloseHandle, HANDLE, STATUS_SINGLE_STEP},
        System::{
            Diagnostics::Debug::{
                AddVectoredExceptionHandler, CONTEXT, CONTEXT_CONTROL_X86,
                CONTEXT_DEBUG_REGISTERS_X86, CONTEXT_INTEGER_X86, EXCEPTION_CONTINUE_EXECUTION,
                EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
            },
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Threading::{
                EVENT_MODIFY_STATE, GetCurrentThreadId, OpenEventW, SYNCHRONIZATION_ACCESS_RIGHTS,
                SetEvent,
            },
        },
    };
    use windows::core::{PCSTR, PCWSTR};

    use crate::{config::PostUnpackGateConfig, themida};

    use super::{DebuggerGateError, GateSnapshot};

    const DR0_ENABLE_MASK: u32 = 0b11;
    const DR0_CONTROL_MASK: u32 = 0b1111 << 16;
    const DR6_BREAKPOINT_ZERO: u32 = 1;
    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;

    static STATE: OnceLock<GateState> = OnceLock::new();
    static TARGET_ADDRESS: AtomicU32 = AtomicU32::new(0);
    static SNAPSHOT: SnapshotSlot = SnapshotSlot::new();

    const SNAPSHOT_EMPTY: u32 = 0;
    const SNAPSHOT_WRITING: u32 = 1;
    const SNAPSHOT_READY: u32 = 2;
    const SNAPSHOT_CONSUMED: u32 = 3;

struct SnapshotSlot {
        state: AtomicU32,
        value: UnsafeCell<MaybeUninit<GateSnapshot>>,
    }

unsafe impl Sync for SnapshotSlot {}

    impl SnapshotSlot {
        const fn new() -> Self {
            Self {
                state: AtomicU32::new(SNAPSHOT_EMPTY),
                value: UnsafeCell::new(MaybeUninit::uninit()),
            }
        }

        fn publish(&self, value: GateSnapshot) {
            if self
                .state
                .compare_exchange(
                    SNAPSHOT_EMPTY,
                    SNAPSHOT_WRITING,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                return;
            }

unsafe { (*self.value.get()).write(value) };
            self.state.store(SNAPSHOT_READY, Ordering::Release);
        }

        fn take(&self) -> Option<GateSnapshot> {
            self.state
                .compare_exchange(
                    SNAPSHOT_READY,
                    SNAPSHOT_CONSUMED,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .ok()?;

Some(unsafe { *(*self.value.get()).assume_init_ref() })
        }
    }

    struct GateState {
        arrived: usize,
        release: usize,
        relative_timeout_100ns: i64,
        target_address: u32,
        nt_wait_for_single_object: usize,
    }

unsafe impl Send for GateState {}
    unsafe impl Sync for GateState {}

    pub(super) fn install(config: &PostUnpackGateConfig) -> Result<(), DebuggerGateError> {
        let (image_base, _image_size) = themida::current_image_layout()?;
        let target = image_base
            .checked_add(config.target_rva as usize)
            .and_then(|address| u32::try_from(address).ok())
            .ok_or(DebuggerGateError::TargetAddressOverflow {
                image_base,
                rva: config.target_rva,
            })?;
        let timeout_100ns = config
            .timeout_ms
            .checked_mul(10_000)
            .and_then(|value| i64::try_from(value).ok())
            .ok_or(DebuggerGateError::TimeoutOverflow(config.timeout_ms))?;
        let nt_wait_for_single_object = resolve_nt_wait_for_single_object()? as usize;
        let arrived_access =
            SYNCHRONIZATION_ACCESS_RIGHTS(EVENT_MODIFY_STATE.0 | SYNCHRONIZE_ACCESS);
        let arrived = open_event(&config.arrived_event, arrived_access)?;
        let release_access = SYNCHRONIZATION_ACCESS_RIGHTS(SYNCHRONIZE_ACCESS);
        let release = match open_event(&config.release_event, release_access) {
            Ok(handle) => handle,
            Err(error) => {
                
                let _ = unsafe { CloseHandle(arrived) };
                return Err(error);
            }
        };

        TARGET_ADDRESS.store(target, Ordering::Release);
        if STATE
            .set(GateState {
                arrived: arrived.0 as usize,
                release: release.0 as usize,
                relative_timeout_100ns: -timeout_100ns,
                target_address: target,
                nt_wait_for_single_object,
            })
            .is_err()
        {
            
            let _ = unsafe { CloseHandle(arrived) };
            let _ = unsafe { CloseHandle(release) };
            return Err(DebuggerGateError::AlreadyInstalled);
        }

let registration = unsafe { AddVectoredExceptionHandler(1, Some(exception_handler)) };
        if registration.is_null() {
            return Err(DebuggerGateError::VehInstall(
                windows::core::Error::from_thread(),
            ));
        }
        Ok(())
    }

    pub(super) fn promote() -> Result<(), DebuggerGateError> {
        if STATE.get().is_none() {
            return Err(DebuggerGateError::NotInstalled);
        }

let registration = unsafe { AddVectoredExceptionHandler(1, Some(exception_handler)) };
        if registration.is_null() {
            return Err(DebuggerGateError::VehInstall(
                windows::core::Error::from_thread(),
            ));
        }
        Ok(())
    }

    pub(super) fn wait_for_snapshot() -> Result<GateSnapshot, DebuggerGateError> {
        let state = STATE.get().ok_or(DebuggerGateError::NotInstalled)?;
        let arrived = HANDLE(state.arrived as *mut c_void);

let nt_wait: NtWaitForSingleObjectFn = unsafe {
            std::mem::transmute::<usize, NtWaitForSingleObjectFn>(state.nt_wait_for_single_object)
        };

let status = unsafe { nt_wait(arrived, 0, std::ptr::null()) };
        if status.0 != 0 {
            return Err(DebuggerGateError::SnapshotWait(status.0));
        }
        SNAPSHOT
            .take()
            .ok_or(DebuggerGateError::SnapshotNotPublished)
    }

    type NtWaitForSingleObjectFn =
        unsafe extern "system" fn(HANDLE, u8, *const i64) -> windows::Win32::Foundation::NTSTATUS;

    fn resolve_nt_wait_for_single_object() -> Result<NtWaitForSingleObjectFn, DebuggerGateError> {
        let ntdll_name = "ntdll.dll\0".encode_utf16().collect::<Vec<_>>();

let ntdll = unsafe { GetModuleHandleW(PCWSTR(ntdll_name.as_ptr())) }
            .map_err(DebuggerGateError::NtWaitResolve)?;
        
        let address =
            unsafe { GetProcAddress(ntdll, PCSTR(c"NtWaitForSingleObject".as_ptr().cast::<u8>())) }
                .ok_or_else(|| {
                    DebuggerGateError::NtWaitResolve(windows::core::Error::from_thread())
                })?;

Ok(unsafe {
            std::mem::transmute::<unsafe extern "system" fn() -> isize, NtWaitForSingleObjectFn>(
                address,
            )
        })
    }

    fn open_event(
        name: &str,
        access: SYNCHRONIZATION_ACCESS_RIGHTS,
    ) -> Result<HANDLE, DebuggerGateError> {
        let mut wide = OsStr::new(name).encode_wide().collect::<Vec<_>>();
        wide.push(0);
        
        unsafe { OpenEventW(access, false, PCWSTR(wide.as_ptr())) }.map_err(|source| {
            DebuggerGateError::EventOpen {
                name: name.to_owned(),
                source,
            }
        })
    }

    unsafe extern "system" fn exception_handler(info: *mut EXCEPTION_POINTERS) -> i32 {
        let Some(state) = STATE.get() else {
            return EXCEPTION_CONTINUE_SEARCH;
        };
        if info.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }

let pointers = unsafe { &mut *info };
        if pointers.ExceptionRecord.is_null() || pointers.ContextRecord.is_null() {
            return EXCEPTION_CONTINUE_SEARCH;
        }

let record = unsafe { &*pointers.ExceptionRecord };
        if record.ExceptionCode != STATUS_SINGLE_STEP
            || record.ExceptionAddress as usize != state.target_address as usize
        {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        
        let context: &mut CONTEXT = unsafe { &mut *pointers.ContextRecord };
        if context.ContextFlags.0 & CONTEXT_DEBUG_REGISTERS_X86.0 != CONTEXT_DEBUG_REGISTERS_X86.0
            || context.ContextFlags.0 & CONTEXT_CONTROL_X86.0 != CONTEXT_CONTROL_X86.0
            || context.ContextFlags.0 & CONTEXT_INTEGER_X86.0 != CONTEXT_INTEGER_X86.0
            || context.Eip != state.target_address
            || context.Dr0 != state.target_address
            || context.Dr6 & DR6_BREAKPOINT_ZERO == 0
            || context.Dr7 & DR0_ENABLE_MASK != 1
            || context.Dr7 & DR0_CONTROL_MASK != 0
        {
            return EXCEPTION_CONTINUE_SEARCH;
        }

SNAPSHOT.publish(GateSnapshot {
            
            primary_thread_id: unsafe { GetCurrentThreadId() },
            eax: context.Eax,
            ebx: context.Ebx,
            ecx: context.Ecx,
            edx: context.Edx,
            esi: context.Esi,
            edi: context.Edi,
            ebp: context.Ebp,
            esp: context.Esp,
            eip: context.Eip,
            eflags: context.EFlags,
            dr6: context.Dr6,
            dr7: context.Dr7,
        });

context.Dr0 = 0;
        context.Dr6 &= !DR6_BREAKPOINT_ZERO;
        context.Dr7 &= !(DR0_ENABLE_MASK | DR0_CONTROL_MASK);
        context.Eip = gate_thunk as *const () as usize as u32;
        context.ContextFlags =
            context.ContextFlags | CONTEXT_CONTROL_X86 | CONTEXT_DEBUG_REGISTERS_X86;
        EXCEPTION_CONTINUE_EXECUTION
    }

    #[unsafe(naked)]
    unsafe extern "system" fn gate_thunk() -> ! {

core::arch::naked_asm!(
            "pushfd",
            "pushad",
            "sub esp, 528",
            "lea eax, [esp + 15]",
            "and eax, -16",
            "fxsave [eax]",
            "call {wait}",
            "lea eax, [esp + 15]",
            "and eax, -16",
            "fxrstor [eax]",
            "add esp, 528",
            "popad",
            "popfd",
            "jmp dword ptr [{target}]",
            wait = sym gate_thunk_wait,
            target = sym TARGET_ADDRESS,
        );
    }

    extern "C" fn gate_thunk_wait() {
        let Some(state) = STATE.get() else {
            return;
        };
        let arrived = HANDLE(state.arrived as *mut c_void);
        let release = HANDLE(state.release as *mut c_void);

let nt_wait: NtWaitForSingleObjectFn = unsafe {
            std::mem::transmute::<usize, NtWaitForSingleObjectFn>(state.nt_wait_for_single_object)
        };

let _ = unsafe { SetEvent(arrived) };
        let _ = unsafe { nt_wait(release, 0, &state.relative_timeout_100ns) };
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn snapshot_slot_preserves_exact_value_and_is_single_use() {
            let slot = SnapshotSlot::new();
            let expected = GateSnapshot {
                primary_thread_id: 0,
                eax: 1,
                ebx: 0,
                ecx: u32::MAX,
                edx: 0x1234_5678,
                esi: 0,
                edi: 7,
                ebp: 0x8000_0000,
                esp: 0,
                eip: 0x00d3_fab0,
                eflags: 0x202,
                dr6: 1,
                dr7: 1,
            };

            assert_eq!(slot.take(), None);
            slot.publish(expected);
            assert_eq!(slot.take(), Some(expected));
            assert_eq!(slot.take(), None);

            slot.publish(GateSnapshot { eax: 2, ..expected });
            assert_eq!(slot.take(), None);
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum DebuggerGateError {
    #[cfg(not(all(windows, target_arch = "x86")))]
    #[error("post-unpack debugger gate requires an i686 Windows shim")]
    UnsupportedTarget,
    #[cfg(all(windows, target_arch = "x86"))]
    #[error(transparent)]
    Image(#[from] crate::themida::ThemidaError),
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("post-unpack target address overflow: image base 0x{image_base:x} + RVA 0x{rva:x}")]
    TargetAddressOverflow { image_base: usize, rva: u32 },
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("could not open post-unpack gate event {name:?}: {source}")]
    EventOpen {
        name: String,
        source: windows::core::Error,
    },
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("the post-unpack debugger gate was initialized more than once")]
    AlreadyInstalled,
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("post-unpack debugger gate promotion ran before installation")]
    NotInstalled,
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("AddVectoredExceptionHandler failed: {0}")]
    VehInstall(windows::core::Error),
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("could not resolve ntdll!NtWaitForSingleObject: {0}")]
    NtWaitResolve(windows::core::Error),
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("post-unpack shim timeout {0} ms does not fit an NT relative timeout")]
    TimeoutOverflow(u64),
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("post-unpack ARRIVED wait failed with NTSTATUS 0x{0:08X}")]
    SnapshotWait(i32),
    #[cfg(all(windows, target_arch = "x86"))]
    #[error("post-unpack ARRIVED was signalled before the register snapshot was published")]
    SnapshotNotPublished,
}
