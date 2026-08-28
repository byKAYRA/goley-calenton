

use std::{ptr, time::{Duration, Instant}};

use tracing::{info, warn};
use windows::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS};

const ROOT_GLOBAL: usize = 0x12BC9C4;

const CONFIG_OFFSET: usize = 0x60;

const GATE_BASE: usize = 0x1F4;

const GATE_COUNT: usize = 6;

const MAX_POLLS: u32 = 200;

const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn patch_config_gates() -> bool {
    let start = Instant::now();

    for attempt in 0..MAX_POLLS {
        let Some(config) = resolve_config_object() else {
            if attempt == 0 {
                info!(
                    event_type = "gate_patch_poll",
                    "config object pointer chain not yet initialised; polling"
                );
            }
            std::thread::sleep(POLL_INTERVAL);
            continue;
        };

        let gates = (config + GATE_BASE) as *mut u8;
        let mut old_protect = PAGE_PROTECTION_FLAGS::default();

let write_result = unsafe {
            VirtualProtect(
                gates.cast(),
                GATE_COUNT,
                PAGE_EXECUTE_READWRITE,
                &mut old_protect,
            )
        };

        if let Err(e) = write_result {
            warn!(
                event_type = "gate_patch_failed",
                attempt,
                error = %e,
                config_addr = config as u64,
                "VirtualProtect failed on gate region"
            );
            std::thread::sleep(POLL_INTERVAL);
            continue;
        }

unsafe {
            for i in 0..GATE_COUNT {
                ptr::write_volatile(gates.add(i), 1u8);
            }
        }

let _ = unsafe {
            VirtualProtect(
                gates.cast(),
                GATE_COUNT,
                old_protect,
                &mut PAGE_PROTECTION_FLAGS::default(),
            )
        };

        let elapsed = start.elapsed();
        info!(
            event_type = "gate_patch_applied",
            attempt,
            config_addr = config as u64,
            gate_base = (config + GATE_BASE) as u64,
            elapsed_ms = elapsed.as_millis() as u64,
            "all 6 category gates patched to 0x01"
        );
        return true;
    }

    warn!(
        event_type = "gate_patch_timeout",
        elapsed_ms = start.elapsed().as_millis() as u64,
        "config object pointer chain never resolved; gate patch not applied"
    );
    false
}

fn resolve_config_object() -> Option<usize> {

let root = unsafe { ptr::read_volatile(ROOT_GLOBAL as *const u32) } as usize;
    if root == 0 {
        return None;
    }

    let config_ptr = (root + CONFIG_OFFSET) as *const u32;
    let config = unsafe { ptr::read_volatile(config_ptr) } as usize;
    if config == 0 {
        return None;
    }

    Some(config)
}
