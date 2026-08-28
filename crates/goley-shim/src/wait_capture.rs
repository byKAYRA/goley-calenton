

use std::{
    cell::Cell,
    collections::HashMap,
    ffi::{CStr, c_void},
    sync::{Mutex, OnceLock},
};

use retour::GenericDetour;
use thiserror::Error;
use tracing::{debug, info, warn};
use windows::core::BOOL;
use windows::{
    Win32::{Foundation::HANDLE, Security::SECURITY_ATTRIBUTES},
    core::{PCSTR, PCWSTR},
};

use crate::{
    gameguard,
    platform::{CallerSite, HookError, capture_caller, resolve_export},
};

const MAX_WAIT_HANDLES: usize = 64;

type CreateEventWFn =
    unsafe extern "system" fn(*const SECURITY_ATTRIBUTES, BOOL, BOOL, PCWSTR) -> HANDLE;
type CreateEventAFn =
    unsafe extern "system" fn(*const SECURITY_ATTRIBUTES, BOOL, BOOL, PCSTR) -> HANDLE;
type OpenEventWFn = unsafe extern "system" fn(u32, BOOL, PCWSTR) -> HANDLE;
type OpenEventAFn = unsafe extern "system" fn(u32, BOOL, PCSTR) -> HANDLE;
type CreateMutexWFn = unsafe extern "system" fn(*const SECURITY_ATTRIBUTES, BOOL, PCWSTR) -> HANDLE;
type CreateMutexAFn = unsafe extern "system" fn(*const SECURITY_ATTRIBUTES, BOOL, PCSTR) -> HANDLE;
type OpenMutexWFn = unsafe extern "system" fn(u32, BOOL, PCWSTR) -> HANDLE;
type OpenMutexAFn = unsafe extern "system" fn(u32, BOOL, PCSTR) -> HANDLE;
type WaitForSingleObjectFn = unsafe extern "system" fn(HANDLE, u32) -> u32;
type WaitForMultipleObjectsFn = unsafe extern "system" fn(u32, *const HANDLE, BOOL, u32) -> u32;
type CloseHandleFn = unsafe extern "system" fn(HANDLE) -> BOOL;
type NtQueryObjectFn = unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> i32;

static CREATE_EVENT_W: OnceLock<GenericDetour<CreateEventWFn>> = OnceLock::new();
static CREATE_EVENT_A: OnceLock<GenericDetour<CreateEventAFn>> = OnceLock::new();
static OPEN_EVENT_W: OnceLock<GenericDetour<OpenEventWFn>> = OnceLock::new();
static OPEN_EVENT_A: OnceLock<GenericDetour<OpenEventAFn>> = OnceLock::new();
static CREATE_MUTEX_W: OnceLock<GenericDetour<CreateMutexWFn>> = OnceLock::new();
static CREATE_MUTEX_A: OnceLock<GenericDetour<CreateMutexAFn>> = OnceLock::new();
static OPEN_MUTEX_W: OnceLock<GenericDetour<OpenMutexWFn>> = OnceLock::new();
static OPEN_MUTEX_A: OnceLock<GenericDetour<OpenMutexAFn>> = OnceLock::new();
static WAIT_SINGLE: OnceLock<GenericDetour<WaitForSingleObjectFn>> = OnceLock::new();
static WAIT_MULTIPLE: OnceLock<GenericDetour<WaitForMultipleObjectsFn>> = OnceLock::new();
static CLOSE_HANDLE: OnceLock<GenericDetour<CloseHandleFn>> = OnceLock::new();
static NT_QUERY_OBJECT: OnceLock<NtQueryObjectFn> = OnceLock::new();
static CAPTURE: OnceLock<WaitCapture> = OnceLock::new();

thread_local! {
    static INSIDE_HOOK: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    
    Event,
    
    Mutex,

Other,
}

impl ObjectKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Event => "event",
            Self::Mutex => "mutex",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedObject {
    
    pub kind: ObjectKind,
    
    pub name: String,
    
    pub type_name: String,
}

#[derive(Debug, Default)]
pub struct WaitCapture {
    objects: Mutex<HashMap<usize, NamedObject>>,
}

impl WaitCapture {
    
    pub fn record(&self, handle: HANDLE, kind: ObjectKind, name: String) {
        if handle.0.is_null() || name.is_empty() {
            return;
        }
        let type_name = match kind {
            ObjectKind::Event => "Event",
            ObjectKind::Mutex => "Mutant",
            ObjectKind::Other => "Unknown",
        };
        self.record_native(handle, kind, name, type_name.to_owned());
    }

pub fn lookup(&self, handle: HANDLE) -> Option<NamedObject> {
        lock_unpoisoned(&self.objects)
            .get(&handle_key(handle))
            .cloned()
    }

pub fn forget(&self, handle: HANDLE) {
        lock_unpoisoned(&self.objects).remove(&handle_key(handle));
    }

    fn record_native(&self, handle: HANDLE, kind: ObjectKind, name: String, type_name: String) {
        if handle.0.is_null() || name.is_empty() {
            return;
        }
        lock_unpoisoned(&self.objects).insert(
            handle_key(handle),
            NamedObject {
                kind,
                name,
                type_name,
            },
        );
    }

    fn lookup_or_query(&self, handle: HANDLE, caller: &CallerSite) -> Option<NamedObject> {
        if let Some(object) = self.lookup(handle) {
            return Some(object);
        }
        let object = query_named_object(handle)?;
        self.record_native(
            handle,
            object.kind,
            object.name.clone(),
            object.type_name.clone(),
        );
        info!(
            event_type = "kernel_object",
            operation = "query_before_wait",
            api = "NtQueryObject",
            object_kind = object.kind.as_str(),
            object_type = %object.type_name,
            object_name = %object.name,
            handle = handle_key(handle) as u64,
            caller_module = %caller.module,
            caller_offset = caller.offset as u64,
            caller_address = caller.address as u64,
            "pre-existing named wait handle discovered"
        );
        Some(object)
    }
}

pub fn initialize() -> Result<(), WaitCaptureError> {
    let address = resolve_export("ntdll.dll", c"NtQueryObject")?;
    
    let query: NtQueryObjectFn = unsafe { std::mem::transmute(address) };
    NT_QUERY_OBJECT
        .set(query)
        .map_err(|_| HookError::AlreadyInitialized("NtQueryObject"))?;
    CAPTURE
        .set(WaitCapture::default())
        .map_err(|_| WaitCaptureError::AlreadyInitialized)
}

pub fn capture() -> Option<&'static WaitCapture> {
    CAPTURE.get()
}

pub fn install_hooks() -> Result<(), WaitCaptureError> {
    install_detour!(
        CREATE_EVENT_W,
        CreateEventWFn,
        "kernel32.dll",
        c"CreateEventW",
        hook_create_event_w
    )?;
    install_detour!(
        CREATE_EVENT_A,
        CreateEventAFn,
        "kernel32.dll",
        c"CreateEventA",
        hook_create_event_a
    )?;
    install_detour!(
        OPEN_EVENT_W,
        OpenEventWFn,
        "kernel32.dll",
        c"OpenEventW",
        hook_open_event_w
    )?;
    install_detour!(
        OPEN_EVENT_A,
        OpenEventAFn,
        "kernel32.dll",
        c"OpenEventA",
        hook_open_event_a
    )?;
    install_detour!(
        CREATE_MUTEX_W,
        CreateMutexWFn,
        "kernel32.dll",
        c"CreateMutexW",
        hook_create_mutex_w
    )?;
    install_detour!(
        CREATE_MUTEX_A,
        CreateMutexAFn,
        "kernel32.dll",
        c"CreateMutexA",
        hook_create_mutex_a
    )?;
    install_detour!(
        OPEN_MUTEX_W,
        OpenMutexWFn,
        "kernel32.dll",
        c"OpenMutexW",
        hook_open_mutex_w
    )?;
    install_detour!(
        OPEN_MUTEX_A,
        OpenMutexAFn,
        "kernel32.dll",
        c"OpenMutexA",
        hook_open_mutex_a
    )?;
    install_detour!(
        WAIT_SINGLE,
        WaitForSingleObjectFn,
        "kernel32.dll",
        c"WaitForSingleObject",
        hook_wait_for_single_object
    )?;
    install_detour!(
        WAIT_MULTIPLE,
        WaitForMultipleObjectsFn,
        "kernel32.dll",
        c"WaitForMultipleObjects",
        hook_wait_for_multiple_objects
    )?;
    install_detour!(
        CLOSE_HANDLE,
        CloseHandleFn,
        "kernel32.dll",
        c"CloseHandle",
        hook_close_handle
    )?;
    Ok(())
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
use install_detour;

unsafe extern "system" fn hook_create_event_w(
    attributes: *const SECURITY_ATTRIBUTES,
    manual_reset: BOOL,
    initial_state: BOOL,
    name: PCWSTR,
) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            CREATE_EVENT_W.get().expect("enabled hook").call(
                attributes,
                manual_reset,
                initial_state,
                name,
            )
        };
    };
    let caller = capture_caller();
    let decoded = wide_name(name);
    let handle = unsafe {
        CREATE_EVENT_W.get().expect("enabled hook").call(
            attributes,
            manual_reset,
            initial_state,
            name,
        )
    };
    observe_open("CreateEventW", ObjectKind::Event, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_create_event_a(
    attributes: *const SECURITY_ATTRIBUTES,
    manual_reset: BOOL,
    initial_state: BOOL,
    name: PCSTR,
) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            CREATE_EVENT_A.get().expect("enabled hook").call(
                attributes,
                manual_reset,
                initial_state,
                name,
            )
        };
    };
    let caller = capture_caller();
    let decoded = ansi_name(name);
    let handle = unsafe {
        CREATE_EVENT_A.get().expect("enabled hook").call(
            attributes,
            manual_reset,
            initial_state,
            name,
        )
    };
    observe_open("CreateEventA", ObjectKind::Event, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_open_event_w(access: u32, inherit: BOOL, name: PCWSTR) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            OPEN_EVENT_W
                .get()
                .expect("enabled hook")
                .call(access, inherit, name)
        };
    };
    let caller = capture_caller();
    let decoded = wide_name(name);
    let handle = unsafe {
        OPEN_EVENT_W
            .get()
            .expect("enabled hook")
            .call(access, inherit, name)
    };
    observe_open("OpenEventW", ObjectKind::Event, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_open_event_a(access: u32, inherit: BOOL, name: PCSTR) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            OPEN_EVENT_A
                .get()
                .expect("enabled hook")
                .call(access, inherit, name)
        };
    };
    let caller = capture_caller();
    let decoded = ansi_name(name);
    let handle = unsafe {
        OPEN_EVENT_A
            .get()
            .expect("enabled hook")
            .call(access, inherit, name)
    };
    observe_open("OpenEventA", ObjectKind::Event, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_create_mutex_w(
    attributes: *const SECURITY_ATTRIBUTES,
    initial_owner: BOOL,
    name: PCWSTR,
) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            CREATE_MUTEX_W
                .get()
                .expect("enabled hook")
                .call(attributes, initial_owner, name)
        };
    };
    let caller = capture_caller();
    let decoded = wide_name(name);
    let handle = unsafe {
        CREATE_MUTEX_W
            .get()
            .expect("enabled hook")
            .call(attributes, initial_owner, name)
    };
    observe_open("CreateMutexW", ObjectKind::Mutex, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_create_mutex_a(
    attributes: *const SECURITY_ATTRIBUTES,
    initial_owner: BOOL,
    name: PCSTR,
) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            CREATE_MUTEX_A
                .get()
                .expect("enabled hook")
                .call(attributes, initial_owner, name)
        };
    };
    let caller = capture_caller();
    let decoded = ansi_name(name);
    let handle = unsafe {
        CREATE_MUTEX_A
            .get()
            .expect("enabled hook")
            .call(attributes, initial_owner, name)
    };
    observe_open("CreateMutexA", ObjectKind::Mutex, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_open_mutex_w(access: u32, inherit: BOOL, name: PCWSTR) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            OPEN_MUTEX_W
                .get()
                .expect("enabled hook")
                .call(access, inherit, name)
        };
    };
    let caller = capture_caller();
    let decoded = wide_name(name);
    let handle = unsafe {
        OPEN_MUTEX_W
            .get()
            .expect("enabled hook")
            .call(access, inherit, name)
    };
    observe_open("OpenMutexW", ObjectKind::Mutex, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_open_mutex_a(access: u32, inherit: BOOL, name: PCSTR) -> HANDLE {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            OPEN_MUTEX_A
                .get()
                .expect("enabled hook")
                .call(access, inherit, name)
        };
    };
    let caller = capture_caller();
    let decoded = ansi_name(name);
    let handle = unsafe {
        OPEN_MUTEX_A
            .get()
            .expect("enabled hook")
            .call(access, inherit, name)
    };
    observe_open("OpenMutexA", ObjectKind::Mutex, handle, decoded, caller);
    handle
}

unsafe extern "system" fn hook_wait_for_single_object(handle: HANDLE, timeout_ms: u32) -> u32 {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            WAIT_SINGLE
                .get()
                .expect("enabled hook")
                .call(handle, timeout_ms)
        };
    };
    let caller = capture_caller();
    let object = capture().and_then(|state| state.lookup_or_query(handle, &caller));
    if let Some(object) = &object {
        signal_selected(handle, object);
        log_wait(WaitLog {
            operation: "wait_enter",
            outcome: "pending",
            api: "WaitForSingleObject",
            objects: std::slice::from_ref(object),
            timeout_ms,
            wait_all: false,
            result: None,
            caller: &caller,
        });
    }
    let result = unsafe {
        WAIT_SINGLE
            .get()
            .expect("enabled hook")
            .call(handle, timeout_ms)
    };
    if let Some(object) = object {
        log_wait(WaitLog {
            operation: "wait_return",
            outcome: "returned",
            api: "WaitForSingleObject",
            objects: &[object],
            timeout_ms,
            wait_all: false,
            result: Some(result),
            caller: &caller,
        });
    }
    result
}

unsafe extern "system" fn hook_wait_for_multiple_objects(
    count: u32,
    handles: *const HANDLE,
    wait_all: BOOL,
    timeout_ms: u32,
) -> u32 {
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            WAIT_MULTIPLE
                .get()
                .expect("enabled hook")
                .call(count, handles, wait_all, timeout_ms)
        };
    };
    let copied = if !handles.is_null() && (count as usize) <= MAX_WAIT_HANDLES {

unsafe { std::slice::from_raw_parts(handles, count as usize) }.to_vec()
    } else {
        Vec::new()
    };
    let caller = capture_caller();
    let observed: Vec<_> = copied
        .iter()
        .filter_map(|&handle| {
            capture()
                .and_then(|state| state.lookup_or_query(handle, &caller))
                .map(|object| (handle, object))
        })
        .collect();
    for (handle, object) in &observed {
        signal_selected(*handle, object);
    }
    if !observed.is_empty() {
        let objects: Vec<_> = observed.iter().map(|(_, object)| object.clone()).collect();
        log_wait(WaitLog {
            operation: "wait_enter",
            outcome: "pending",
            api: "WaitForMultipleObjects",
            objects: &objects,
            timeout_ms,
            wait_all: wait_all.as_bool(),
            result: None,
            caller: &caller,
        });
    }
    let result = unsafe {
        WAIT_MULTIPLE
            .get()
            .expect("enabled hook")
            .call(count, handles, wait_all, timeout_ms)
    };
    if !observed.is_empty() {
        let objects: Vec<_> = observed.into_iter().map(|(_, object)| object).collect();
        log_wait(WaitLog {
            operation: "wait_return",
            outcome: "returned",
            api: "WaitForMultipleObjects",
            objects: &objects,
            timeout_ms,
            wait_all: wait_all.as_bool(),
            result: Some(result),
            caller: &caller,
        });
    }
    result
}

unsafe extern "system" fn hook_close_handle(handle: HANDLE) -> BOOL {
    let Some(_scope) = HookScope::enter() else {
        return unsafe { CLOSE_HANDLE.get().expect("enabled hook").call(handle) };
    };
    let result = unsafe { CLOSE_HANDLE.get().expect("enabled hook").call(handle) };
    if result.as_bool()
        && let Some(state) = capture()
    {
        state.forget(handle);
    }
    result
}

fn observe_open(
    api: &'static str,
    kind: ObjectKind,
    handle: HANDLE,
    name: Option<String>,
    caller: CallerSite,
) {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return;
    };
    if let Some(state) = capture() {
        state.record(handle, kind, name.clone());
    }
    let signalled = if kind == ObjectKind::Event {
        signal_selected(
            handle,
            &NamedObject {
                kind,
                name: name.clone(),
                type_name: match kind {
                    ObjectKind::Event => "Event",
                    ObjectKind::Mutex => "Mutant",
                    ObjectKind::Other => "Unknown",
                }
                .to_owned(),
            },
        )
    } else {
        false
    };
    info!(
        event_type = "kernel_object",
        operation = "open_or_create",
        api,
        object_kind = kind.as_str(),
        object_type = match kind {
            ObjectKind::Event => "Event",
            ObjectKind::Mutex => "Mutant",
            ObjectKind::Other => "Unknown",
        },
        object_name = %name,
        handle = handle_key(handle) as u64,
        caller_module = %caller.module,
        caller_offset = caller.offset as u64,
        caller_address = caller.address as u64,
        gameguard_signalled = signalled,
        "named kernel object observed"
    );
}

fn signal_selected(handle: HANDLE, object: &NamedObject) -> bool {
    if object.kind != ObjectKind::Event {
        return false;
    }
    let Some(controller) = gameguard::controller() else {
        return false;
    };
    match controller.signal_if_selected(handle, &object.name) {
        Ok(signalled) => {
            if signalled {
                debug!(
                    event_type = "gameguard_signal",
                    object_name = %object.name,
                    handle = handle_key(handle) as u64,
                    "selected ready event signalled"
                );
            }
            signalled
        }
        Err(error) => {
            warn!(
                event_type = "gameguard_signal_error",
                object_name = %object.name,
                error = %error,
                "selected ready event could not be signalled"
            );
            false
        }
    }
}

struct WaitLog<'a> {
    operation: &'static str,
    outcome: &'static str,
    api: &'static str,
    objects: &'a [NamedObject],
    timeout_ms: u32,
    wait_all: bool,
    result: Option<u32>,
    caller: &'a CallerSite,
}

fn log_wait(wait: WaitLog<'_>) {
    let names: Vec<&str> = wait
        .objects
        .iter()
        .map(|object| object.name.as_str())
        .collect();
    let kinds: Vec<&str> = wait
        .objects
        .iter()
        .map(|object| object.kind.as_str())
        .collect();
    let types: Vec<&str> = wait
        .objects
        .iter()
        .map(|object| object.type_name.as_str())
        .collect();
    info!(
        event_type = "kernel_wait",
        operation = wait.operation,
        outcome = wait.outcome,
        api = wait.api,
        object_names = ?names,
        object_kinds = ?kinds,
        object_types = ?types,
        timeout_ms = wait.timeout_ms,
        wait_all = wait.wait_all,
        wait_result = wait.result,
        caller_module = %wait.caller.module,
        caller_offset = wait.caller.offset as u64,
        caller_address = wait.caller.address as u64,
        "named kernel object wait observed"
    );
}

fn handle_key(handle: HANDLE) -> usize {
    handle.0 as usize
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn wide_name(name: PCWSTR) -> Option<String> {
    if name.is_null() {
        return None;
    }

unsafe { name.to_string().ok() }
}

fn ansi_name(name: PCSTR) -> Option<String> {
    if name.is_null() {
        return None;
    }

let bytes = unsafe { CStr::from_ptr(name.as_ptr().cast()) }.to_bytes();
    Some(String::from_utf8_lossy(bytes).into_owned())
}

#[repr(C)]
struct NativeUnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

fn query_named_object(handle: HANDLE) -> Option<NamedObject> {

let name = query_object_unicode_string(handle, 1)?;
    if name.is_empty() {
        return None;
    }
    let type_name = query_object_unicode_string(handle, 2).unwrap_or_else(|| "Unknown".to_owned());
    let kind = if type_name.eq_ignore_ascii_case("Event") {
        ObjectKind::Event
    } else if type_name.eq_ignore_ascii_case("Mutant") || type_name.eq_ignore_ascii_case("Mutex") {
        ObjectKind::Mutex
    } else {
        ObjectKind::Other
    };
    Some(NamedObject {
        kind,
        name,
        type_name,
    })
}

fn query_object_unicode_string(handle: HANDLE, information_class: u32) -> Option<String> {
    const INITIAL_BYTES: usize = 512;
    const MAX_BYTES: usize = 64 * 1024;
    let query = *NT_QUERY_OBJECT.get()?;
    let mut capacity = INITIAL_BYTES;

    for _ in 0..4 {
        let words = capacity.div_ceil(std::mem::size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let byte_capacity = storage.len() * std::mem::size_of::<usize>();
        let mut required = 0_u32;

let status = unsafe {
            query(
                handle,
                information_class,
                storage.as_mut_ptr().cast(),
                byte_capacity as u32,
                &mut required,
            )
        };
        if status >= 0 {
            let header = storage.as_ptr().cast::<NativeUnicodeString>();

let string = unsafe { &*header };
            if string.length == 0 || string.length % 2 != 0 || string.buffer.is_null() {
                return None;
            }
            let start = storage.as_ptr() as usize;
            let end = start.checked_add(byte_capacity)?;
            let text_start = string.buffer as usize;
            let text_end = text_start.checked_add(string.length as usize)?;
            if text_start < start || text_end > end {
                return None;
            }

let units =
                unsafe { std::slice::from_raw_parts(string.buffer, string.length as usize / 2) };
            return Some(String::from_utf16_lossy(units));
        }
        let requested = required as usize;
        if requested <= capacity || requested > MAX_BYTES {
            return None;
        }
        capacity = requested.next_power_of_two().min(MAX_BYTES);
    }
    None
}

struct HookScope;

impl HookScope {
    fn enter() -> Option<Self> {
        INSIDE_HOOK.with(|inside| {
            if inside.replace(true) {
                None
            } else {
                Some(Self)
            }
        })
    }
}

impl Drop for HookScope {
    fn drop(&mut self) {
        INSIDE_HOOK.with(|inside| inside.set(false));
    }
}

#[derive(Debug, Error)]
pub enum WaitCaptureError {
    
    #[error("wait capture state was already initialized")]
    AlreadyInitialized,
    
    #[error(transparent)]
    Hook(#[from] HookError),
}
