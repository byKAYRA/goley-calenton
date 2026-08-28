

use std::{
    ffi::{CStr, c_void},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use thiserror::Error;
use windows::{
    Win32::{
        Foundation::HMODULE,
        System::{
            Diagnostics::Debug::RtlCaptureStackBackTrace,
            LibraryLoader::{GetModuleFileNameW, GetModuleHandleW, GetProcAddress},
            Memory::{MEMORY_BASIC_INFORMATION, VirtualQuery},
        },
    },
    core::{PCSTR, PCWSTR},
};

static SHIM_MODULE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallerSite {
    
    pub module: String,
    
    pub offset: usize,
    
    pub address: usize,
}

pub(crate) fn set_shim_module(module: usize) {
    SHIM_MODULE.store(module, Ordering::Release);
}

pub(crate) fn capture_caller() -> CallerSite {
    let mut frames = [std::ptr::null_mut::<c_void>(); 32];

let captured = unsafe { RtlCaptureStackBackTrace(0, &mut frames, None) } as usize;
    let shim = SHIM_MODULE.load(Ordering::Acquire);

    for &frame in &frames[..captured] {
        let address = frame as usize;
        let mut info = MEMORY_BASIC_INFORMATION::default();

let queried = unsafe {
            VirtualQuery(
                Some(frame.cast_const()),
                &mut info,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if queried == 0 || info.AllocationBase.is_null() {
            continue;
        }
        let base = info.AllocationBase as usize;
        if base == shim {
            continue;
        }
        let module = module_name(HMODULE(info.AllocationBase));
        if module.is_empty() {

continue;
        }
        return CallerSite {
            module,
            offset: address.saturating_sub(base),
            address,
        };
    }

    CallerSite {
        module: "<unknown>".to_owned(),
        offset: 0,
        address: 0,
    }
}

fn module_name(module: HMODULE) -> String {
    let mut buffer = [0_u16; 1024];

let length = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    if length == 0 {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buffer[..length.min(buffer.len())]);
    Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&path)
        .to_owned()
}

pub(crate) fn resolve_export(
    module: &str,
    symbol: &'static CStr,
) -> Result<*const c_void, HookError> {
    let mut wide: Vec<u16> = module.encode_utf16().collect();
    wide.push(0);
    
    let image =
        unsafe { GetModuleHandleW(PCWSTR(wide.as_ptr())) }.map_err(|error| HookError::Resolve {
            module: module.to_owned(),
            symbol: symbol.to_string_lossy().into_owned(),
            detail: error.to_string(),
        })?;

let proc = unsafe { GetProcAddress(image, PCSTR(symbol.as_ptr().cast())) };
    proc.map(|function| function as *const c_void)
        .ok_or_else(|| HookError::Resolve {
            module: module.to_owned(),
            symbol: symbol.to_string_lossy().into_owned(),
            detail: "export not found".to_owned(),
        })
}

#[derive(Debug, Error)]
pub enum HookError {
    
    #[error("could not resolve {module}!{symbol}: {detail}")]
    Resolve {
        
        module: String,
        
        symbol: String,
        
        detail: String,
    },
    
    #[error("detour operation failed for {symbol}: {detail}")]
    Detour {
        
        symbol: &'static str,
        
        detail: String,
    },
    
    #[error("hook {0} was already initialized")]
    AlreadyInitialized(&'static str),
}
