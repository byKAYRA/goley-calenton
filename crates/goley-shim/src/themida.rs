

use std::{
    ptr, thread,
    time::{Duration, Instant},
};

use thiserror::Error;
use windows::Win32::{
    Foundation::HMODULE,
    System::{
        LibraryLoader::GetModuleHandleW,
        Memory::{
            MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ,
            PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS,
            VirtualQuery,
        },
    },
};

use crate::config::UnpackConfig;

const DOS_MAGIC: u16 = 0x5a4d;
const PE_MAGIC: u32 = 0x0000_4550;
const PROBE_BYTES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnpackReadiness {
    
    pub probe_rva: u32,

pub measured_rva: bool,
    
    pub sample: [u8; PROBE_BYTES],
    
    pub elapsed: Duration,
}

pub fn wait_until_ready(config: &UnpackConfig) -> Result<UnpackReadiness, ThemidaError> {
    
    let module = unsafe { GetModuleHandleW(None) }.map_err(ThemidaError::Windows)?;
    let base = module.0 as usize;
    let pe = read_pe_layout(module)?;
    let (probe_rva, measured_rva) = config
        .oep_rva
        .map(|rva| (rva, true))
        .unwrap_or((pe.entry_rva, false));

    let address = (base + probe_rva as usize) as *const u8;
    let started = Instant::now();
    let timeout = Duration::from_millis(config.timeout_ms);
    let interval = Duration::from_millis(config.poll_interval_ms);
    let mut previous = None;
    let mut stable = 0_u32;

    while started.elapsed() < timeout {
        if let Some(sample) = sample_executable_page(address) {
            if sample[0] != 0xcc {
                if previous == Some(sample) {
                    stable = stable.saturating_add(1);
                } else {
                    previous = Some(sample);
                    stable = 1;
                }
                if stable >= config.stable_samples {
                    if config.post_ready_delay_ms != 0 {
                        thread::sleep(Duration::from_millis(config.post_ready_delay_ms));
                    }
                    return Ok(UnpackReadiness {
                        probe_rva,
                        measured_rva,
                        sample,
                        elapsed: started.elapsed(),
                    });
                }
            } else {
                previous = None;
                stable = 0;
            }
        } else {
            previous = None;
            stable = 0;
        }
        thread::sleep(interval);
    }

    Err(ThemidaError::Timeout {
        rva: probe_rva,
        timeout,
    })
}

fn sample_executable_page(address: *const u8) -> Option<[u8; PROBE_BYTES]> {
    let mut info = MEMORY_BASIC_INFORMATION::default();
    
    let queried = unsafe {
        VirtualQuery(
            Some(address.cast()),
            &mut info,
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    if queried == 0 || info.State != MEM_COMMIT {
        return None;
    }
    let protection = info.Protect;
    if protection.contains(PAGE_GUARD) || protection.contains(PAGE_NOACCESS) {
        return None;
    }
    let executable = protection.contains(PAGE_EXECUTE)
        || protection.contains(PAGE_EXECUTE_READ)
        || protection.contains(PAGE_EXECUTE_READWRITE)
        || protection.contains(PAGE_EXECUTE_WRITECOPY);
    if !executable {
        return None;
    }
    let region_start = info.BaseAddress as usize;
    let region_end = region_start.checked_add(info.RegionSize)?;
    if (address as usize).checked_add(PROBE_BYTES)? > region_end {
        return None;
    }
    let mut bytes = [0_u8; PROBE_BYTES];

unsafe { ptr::copy_nonoverlapping(address, bytes.as_mut_ptr(), PROBE_BYTES) };
    Some(bytes)
}

#[derive(Clone, Copy, Debug)]
struct PeLayout {
    entry_rva: u32,
    image_size: u32,
}

pub(crate) fn current_image_layout() -> Result<(usize, u32), ThemidaError> {
    
    let module = unsafe { GetModuleHandleW(None) }.map_err(ThemidaError::Windows)?;
    let layout = read_pe_layout(module)?;
    Ok((module.0 as usize, layout.image_size))
}

fn read_pe_layout(module: HMODULE) -> Result<PeLayout, ThemidaError> {
    let base = module.0 as *const u8;
    if base.is_null() {
        return Err(ThemidaError::InvalidPe("null image base"));
    }

unsafe {
        if ptr::read_unaligned(base.cast::<u16>()) != DOS_MAGIC {
            return Err(ThemidaError::InvalidPe("missing MZ signature"));
        }
        let nt_offset = ptr::read_unaligned(base.add(0x3c).cast::<u32>()) as usize;
        if nt_offset > 16 * 1024 * 1024 {
            return Err(ThemidaError::InvalidPe("implausible e_lfanew"));
        }
        let nt = base.add(nt_offset);
        if ptr::read_unaligned(nt.cast::<u32>()) != PE_MAGIC {
            return Err(ThemidaError::InvalidPe("missing PE signature"));
        }
        let optional = nt.add(24);
        let optional_magic = ptr::read_unaligned(optional.cast::<u16>());
        if optional_magic != 0x10b && optional_magic != 0x20b {
            return Err(ThemidaError::InvalidPe("unsupported optional-header magic"));
        }
        let entry_rva = ptr::read_unaligned(optional.add(16).cast::<u32>());
        let image_size = ptr::read_unaligned(optional.add(56).cast::<u32>());
        if image_size == 0 || entry_rva >= image_size {
            return Err(ThemidaError::InvalidPe("invalid image extent"));
        }
        Ok(PeLayout {
            entry_rva,
            image_size,
        })
    }
}

#[derive(Debug, Error)]
pub enum ThemidaError {
    
    #[error("Windows module query failed: {0}")]
    Windows(windows::core::Error),
    
    #[error("invalid mapped PE image: {0}")]
    InvalidPe(&'static str),
    
    #[error("unpack readiness timed out at RVA 0x{rva:x} after {timeout:?}")]
    Timeout {
        
        rva: u32,
        
        timeout: Duration,
    },
}
