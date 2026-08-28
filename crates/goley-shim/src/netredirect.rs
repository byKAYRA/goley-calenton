

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

#[cfg(feature = "netredirect")]
use std::{cell::Cell, ffi::c_void, mem, ptr, sync::OnceLock};

#[cfg(feature = "netredirect")]
use retour::GenericDetour;
use thiserror::Error;
#[cfg(feature = "netredirect")]
use tracing::info;
#[cfg(feature = "netredirect")]
use windows::{
    Win32::{
        Networking::WinSock::{
            AF_INET, QOS, SIO_GET_EXTENSION_FUNCTION_POINTER, SOCKADDR, SOCKET, WSABUF,
            WSAID_CONNECTEX,
        },
        System::LibraryLoader::LoadLibraryW,
    },
    core::{GUID, w},
};

#[cfg(feature = "netredirect")]
use crate::platform::{HookError, capture_caller, resolve_export};

#[cfg(feature = "netredirect")]
const MEASURED_ENTRY_IP: Ipv4Addr = Ipv4Addr::new(213, 74, 179, 12);
#[cfg(feature = "netredirect")]
const MEASURED_ENTRY_PORT: u16 = 2270;
#[cfg(feature = "netredirect")]
const MEASURED_AUTH_PORT: u16 = 8000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetRedirectState {
    
    FeatureDisabled,
    
    ConfigurationDisabled,

Installed {
        
        original: SocketAddrV4,
        
        replacement: SocketAddrV4,
    },
}

pub fn initialize(entry: Option<&str>) -> Result<NetRedirectState, NetRedirectError> {
    let Some(entry) = entry else {
        return Ok(if cfg!(feature = "netredirect") {
            NetRedirectState::ConfigurationDisabled
        } else {
            NetRedirectState::FeatureDisabled
        });
    };

    let replacement = parse_replacement(entry)?;

    #[cfg(not(feature = "netredirect"))]
    {
        let _ = replacement;
        Err(NetRedirectError::FeatureUnavailable)
    }

    #[cfg(feature = "netredirect")]
    {
        let rule = RedirectRule {
            original: measured_entry(),
            replacement,
        };
        install_hooks(rule)?;
        Ok(NetRedirectState::Installed {
            original: rule.original,
            replacement: rule.replacement,
        })
    }
}

pub(crate) fn validate_entry(entry: &str) -> Result<(), NetRedirectError> {
    let _ = parse_replacement(entry)?;
    if cfg!(feature = "netredirect") {
        Ok(())
    } else {
        Err(NetRedirectError::FeatureUnavailable)
    }
}

fn parse_replacement(entry: &str) -> Result<SocketAddrV4, NetRedirectError> {
    let endpoint =
        entry
            .parse::<SocketAddr>()
            .map_err(|error| NetRedirectError::InvalidEndpoint {
                endpoint: entry.to_owned(),
                detail: error.to_string(),
            })?;
    let SocketAddr::V4(endpoint) = endpoint else {
        return Err(NetRedirectError::Ipv4LoopbackRequired(entry.to_owned()));
    };
    if endpoint.ip() != &Ipv4Addr::LOCALHOST {
        return Err(NetRedirectError::Ipv4LoopbackRequired(entry.to_owned()));
    }
    if endpoint.port() == 0 {
        return Err(NetRedirectError::NonZeroPortRequired);
    }
    Ok(endpoint)
}

#[cfg(feature = "netredirect")]
const fn measured_entry() -> SocketAddrV4 {
    SocketAddrV4::new(MEASURED_ENTRY_IP, MEASURED_ENTRY_PORT)
}

#[cfg(feature = "netredirect")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RedirectRule {
    original: SocketAddrV4,
    replacement: SocketAddrV4,
}

#[cfg(feature = "netredirect")]
impl RedirectRule {
    fn replacement_for(self, destination: SocketAddrV4) -> Option<SocketAddrV4> {
        if destination.ip() == &MEASURED_ENTRY_IP {
            match destination.port() {
                MEASURED_AUTH_PORT | MEASURED_ENTRY_PORT | 2271 | 2272 => Some(SocketAddrV4::new(
                    *self.replacement.ip(),
                    destination.port(),
                )),
                _ => None,
            }
        } else {
            None
        }
    }
}

#[cfg(feature = "netredirect")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RawSockAddrIn {
    family: u16,
    port_network_order: u16,
    address: [u8; 4],
    zero: [u8; 8],
}

#[cfg(feature = "netredirect")]
impl RawSockAddrIn {
    fn socket_addr(self) -> Option<SocketAddrV4> {
        (self.family == AF_INET.0).then(|| {
            SocketAddrV4::new(
                Ipv4Addr::from(self.address),
                u16::from_be(self.port_network_order),
            )
        })
    }

    fn with_destination(mut self, destination: SocketAddrV4) -> Self {
        self.port_network_order = destination.port().to_be();
        self.address = destination.ip().octets();
        self
    }
}

#[cfg(feature = "netredirect")]
type ConnectFn = unsafe extern "system" fn(SOCKET, *const SOCKADDR, i32) -> i32;
#[cfg(feature = "netredirect")]
type WsaConnectFn = unsafe extern "system" fn(
    SOCKET,
    *const SOCKADDR,
    i32,
    *const WSABUF,
    *mut WSABUF,
    *const QOS,
    *const QOS,
) -> i32;
#[cfg(feature = "netredirect")]
type WsaIoctlFn = unsafe extern "system" fn(
    SOCKET,
    u32,
    *const c_void,
    u32,
    *mut c_void,
    u32,
    *mut u32,
    *mut c_void,
    *const c_void,
) -> i32;

#[cfg(feature = "netredirect")]
static RULE: OnceLock<RedirectRule> = OnceLock::new();
#[cfg(feature = "netredirect")]
static CONNECT: OnceLock<GenericDetour<ConnectFn>> = OnceLock::new();
#[cfg(feature = "netredirect")]
static WSA_CONNECT: OnceLock<GenericDetour<WsaConnectFn>> = OnceLock::new();
#[cfg(feature = "netredirect")]
static WSA_IOCTL: OnceLock<GenericDetour<WsaIoctlFn>> = OnceLock::new();

#[cfg(feature = "netredirect")]
thread_local! {
    static INSIDE_HOOK: Cell<bool> = const { Cell::new(false) };
}

#[cfg(feature = "netredirect")]
fn install_hooks(rule: RedirectRule) -> Result<(), NetRedirectError> {
    ensure_winsock_loaded()?;

    let connect_address = resolve_export("ws2_32.dll", c"connect")?;

let connect_target: ConnectFn = unsafe { mem::transmute(connect_address) };
    
    let connect = unsafe { GenericDetour::new(connect_target, hook_connect) }.map_err(|error| {
        HookError::Detour {
            symbol: "connect",
            detail: error.to_string(),
        }
    })?;

    let wsa_connect_address = resolve_export("ws2_32.dll", c"WSAConnect")?;

let wsa_connect_target: WsaConnectFn = unsafe { mem::transmute(wsa_connect_address) };
    
    let wsa_connect =
        unsafe { GenericDetour::new(wsa_connect_target, hook_wsa_connect) }.map_err(|error| {
            HookError::Detour {
                symbol: "WSAConnect",
                detail: error.to_string(),
            }
        })?;

    let wsa_ioctl_address = resolve_export("ws2_32.dll", c"WSAIoctl")?;

let wsa_ioctl_target: WsaIoctlFn = unsafe { mem::transmute(wsa_ioctl_address) };
    
    let wsa_ioctl =
        unsafe { GenericDetour::new(wsa_ioctl_target, hook_wsa_ioctl) }.map_err(|error| {
            HookError::Detour {
                symbol: "WSAIoctl",
                detail: error.to_string(),
            }
        })?;

    RULE.set(rule)
        .map_err(|_| NetRedirectError::AlreadyInitialized("redirect rule"))?;
    CONNECT
        .set(connect)
        .map_err(|_| NetRedirectError::AlreadyInitialized("connect"))?;
    WSA_CONNECT
        .set(wsa_connect)
        .map_err(|_| NetRedirectError::AlreadyInitialized("WSAConnect"))?;
    WSA_IOCTL
        .set(wsa_ioctl)
        .map_err(|_| NetRedirectError::AlreadyInitialized("WSAIoctl"))?;

unsafe { CONNECT.get().expect("connect slot initialized").enable() }.map_err(|error| {
        HookError::Detour {
            symbol: "connect",
            detail: error.to_string(),
        }
    })?;
    if let Err(error) = unsafe {
        WSA_CONNECT
            .get()
            .expect("WSAConnect slot initialized")
            .enable()
    } {

let rollback = unsafe { CONNECT.get().expect("connect slot initialized").disable() };
        return match rollback {
            Ok(()) => Err(HookError::Detour {
                symbol: "WSAConnect",
                detail: error.to_string(),
            }
            .into()),
            Err(rollback_error) => Err(NetRedirectError::Rollback {
                install_detail: error.to_string(),
                rollback_detail: rollback_error.to_string(),
            }),
        };
    }

    if let Err(error) = unsafe { WSA_IOCTL.get().expect("WSAIoctl slot initialized").enable() } {

let wsa_connect_rollback = unsafe {
            WSA_CONNECT
                .get()
                .expect("WSAConnect slot initialized")
                .disable()
        };
        
        let connect_rollback =
            unsafe { CONNECT.get().expect("connect slot initialized").disable() };
        let rollback_detail = match (wsa_connect_rollback, connect_rollback) {
            (Ok(()), Ok(())) => {
                return Err(HookError::Detour {
                    symbol: "WSAIoctl",
                    detail: error.to_string(),
                }
                .into());
            }
            (wsa, connect) => format!("WSAConnect={wsa:?}; connect={connect:?}"),
        };
        return Err(NetRedirectError::Rollback {
            install_detail: format!("WSAIoctl: {error}"),
            rollback_detail,
        });
    }

    Ok(())
}

#[cfg(feature = "netredirect")]
fn ensure_winsock_loaded() -> Result<(), NetRedirectError> {

let _retained_module = unsafe { LoadLibraryW(w!("ws2_32.dll")) }
        .map_err(|error| NetRedirectError::WinsockLoad(error.to_string()))?;
    Ok(())
}

#[cfg(feature = "netredirect")]
unsafe extern "system" fn hook_connect(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_length: i32,
) -> i32 {
    let trampoline = CONNECT.get().expect("enabled connect hook");
    let Some(_scope) = HookScope::enter() else {
        return unsafe { trampoline.call(socket, name, name_length) };
    };

    if let Some(attempt) = observe_ipv4_address(name, name_length) {
        log_connect_attempt("connect", socket, attempt.original, attempt.replacement);
        if let Some(replacement) = attempt.replacement {
            let redirected = attempt.raw.with_destination(replacement);
            return unsafe {
                trampoline.call(
                    socket,
                    ptr::from_ref(&redirected).cast::<SOCKADDR>(),
                    size_of_sockaddr_in(),
                )
            };
        }
    }

unsafe { trampoline.call(socket, name, name_length) }
}

#[cfg(feature = "netredirect")]
unsafe extern "system" fn hook_wsa_connect(
    socket: SOCKET,
    name: *const SOCKADDR,
    name_length: i32,
    caller_data: *const WSABUF,
    callee_data: *mut WSABUF,
    send_qos: *const QOS,
    group_qos: *const QOS,
) -> i32 {
    let trampoline = WSA_CONNECT.get().expect("enabled WSAConnect hook");
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            trampoline.call(
                socket,
                name,
                name_length,
                caller_data,
                callee_data,
                send_qos,
                group_qos,
            )
        };
    };

    if let Some(attempt) = observe_ipv4_address(name, name_length) {
        log_connect_attempt("WSAConnect", socket, attempt.original, attempt.replacement);
        if let Some(replacement) = attempt.replacement {
            let redirected = attempt.raw.with_destination(replacement);
            return unsafe {
                trampoline.call(
                    socket,
                    ptr::from_ref(&redirected).cast::<SOCKADDR>(),
                    size_of_sockaddr_in(),
                    caller_data,
                    callee_data,
                    send_qos,
                    group_qos,
                )
            };
        }
    }

unsafe {
        trampoline.call(
            socket,
            name,
            name_length,
            caller_data,
            callee_data,
            send_qos,
            group_qos,
        )
    }
}

#[cfg(feature = "netredirect")]
unsafe extern "system" fn hook_wsa_ioctl(
    socket: SOCKET,
    io_control_code: u32,
    input_buffer: *const c_void,
    input_length: u32,
    output_buffer: *mut c_void,
    output_length: u32,
    bytes_returned: *mut u32,
    overlapped: *mut c_void,
    completion_routine: *const c_void,
) -> i32 {
    let trampoline = WSA_IOCTL.get().expect("enabled WSAIoctl hook");
    let Some(_scope) = HookScope::enter() else {
        return unsafe {
            trampoline.call(
                socket,
                io_control_code,
                input_buffer,
                input_length,
                output_buffer,
                output_length,
                bytes_returned,
                overlapped,
                completion_routine,
            )
        };
    };

    let queried_guid = read_extension_guid(io_control_code, input_buffer, input_length);
    let result = unsafe {
        trampoline.call(
            socket,
            io_control_code,
            input_buffer,
            input_length,
            output_buffer,
            output_length,
            bytes_returned,
            overlapped,
            completion_routine,
        )
    };

    if let Some(guid) = queried_guid {
        let returned_pointer = read_extension_pointer(result, output_buffer, output_length);
        log_extension_query(socket, guid, returned_pointer, result);
    }

    result
}

#[cfg(feature = "netredirect")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Ipv4Attempt {
    raw: RawSockAddrIn,
    original: SocketAddrV4,
    replacement: Option<SocketAddrV4>,
}

#[cfg(feature = "netredirect")]
fn observe_ipv4_address(name: *const SOCKADDR, name_length: i32) -> Option<Ipv4Attempt> {
    let raw = read_ipv4(name, name_length)?;
    classify_ipv4(
        *RULE.get().expect("redirect rule initialized before hooks"),
        raw,
    )
}

#[cfg(feature = "netredirect")]
fn classify_ipv4(rule: RedirectRule, raw: RawSockAddrIn) -> Option<Ipv4Attempt> {
    let original = raw.socket_addr()?;
    Some(Ipv4Attempt {
        raw,
        original,
        replacement: rule.replacement_for(original),
    })
}

#[cfg(feature = "netredirect")]
fn read_ipv4(name: *const SOCKADDR, name_length: i32) -> Option<RawSockAddrIn> {
    if name.is_null() || name_length < size_of_sockaddr_in() {
        return None;
    }

Some(unsafe { ptr::read_unaligned(name.cast::<RawSockAddrIn>()) })
}

#[cfg(feature = "netredirect")]
fn size_of_sockaddr_in() -> i32 {
    i32::try_from(mem::size_of::<RawSockAddrIn>()).expect("sockaddr_in size fits i32")
}

#[cfg(feature = "netredirect")]
fn read_extension_guid(
    io_control_code: u32,
    input_buffer: *const c_void,
    input_length: u32,
) -> Option<GUID> {
    if io_control_code != SIO_GET_EXTENSION_FUNCTION_POINTER
        || input_buffer.is_null()
        || input_length < u32::try_from(mem::size_of::<GUID>()).expect("GUID size fits u32")
    {
        return None;
    }

Some(unsafe { ptr::read_unaligned(input_buffer.cast::<GUID>()) })
}

#[cfg(feature = "netredirect")]
fn read_extension_pointer(
    call_result: i32,
    output_buffer: *mut c_void,
    output_length: u32,
) -> Option<usize> {
    if call_result != 0
        || output_buffer.is_null()
        || output_length < u32::try_from(mem::size_of::<usize>()).expect("pointer size fits u32")
    {
        return None;
    }

Some(unsafe { ptr::read_unaligned(output_buffer.cast::<usize>()) })
}

#[cfg(feature = "netredirect")]
fn log_connect_attempt(
    api: &'static str,
    socket: SOCKET,
    original: SocketAddrV4,
    replacement: Option<SocketAddrV4>,
) {
    let caller = capture_caller();
    if let Some(replacement) = replacement {
        info!(
            event_type = "network_connect_attempt",
            api,
            socket = socket.0 as u64,
            matched = true,
            original_destination = %original,
            redirected_destination = %replacement,
            caller_module = %caller.module,
            caller_offset = caller.offset as u64,
            caller_address = caller.address as u64,
            "valid IPv4 connection attempt matched the measured entry route"
        );

info!(
            event_type = "network_connect_redirect",
            api,
            socket = socket.0 as u64,
            matched = true,
            original_destination = %original,
            redirected_destination = %replacement,
            caller_module = %caller.module,
            caller_offset = caller.offset as u64,
            caller_address = caller.address as u64,
            "allowlisted entry-server connection redirected to the local listener"
        );
    } else {
        info!(
            event_type = "network_connect_attempt",
            api,
            socket = socket.0 as u64,
            matched = false,
            original_destination = %original,
            caller_module = %caller.module,
            caller_offset = caller.offset as u64,
            caller_address = caller.address as u64,
            "valid nonmatching IPv4 connection attempt passed through unchanged"
        );
    }
}

#[cfg(feature = "netredirect")]
fn log_extension_query(
    socket: SOCKET,
    guid: GUID,
    returned_pointer: Option<usize>,
    call_result: i32,
) {
    let caller = capture_caller();
    let guid_text = format!("{guid:?}");
    let returned_pointer = returned_pointer.unwrap_or(0);
    info!(
        event_type = "network_extension_query",
        api = "WSAIoctl",
        socket = socket.0 as u64,
        extension_guid = %guid_text,
        connect_ex = guid == WSAID_CONNECTEX,
        call_result = call_result as i64,
        returned_pointer = returned_pointer as u64,
        returned_pointer_valid = returned_pointer != 0,
        caller_module = %caller.module,
        caller_offset = caller.offset as u64,
        caller_address = caller.address as u64,
        "Winsock extension-function query observed without changing its result"
    );
}

#[cfg(feature = "netredirect")]
struct HookScope;

#[cfg(feature = "netredirect")]
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

#[cfg(feature = "netredirect")]
impl Drop for HookScope {
    fn drop(&mut self) {
        INSIDE_HOOK.with(|inside| inside.set(false));
    }
}

#[derive(Debug, Error)]
pub enum NetRedirectError {
    
    #[error("invalid entry redirect endpoint {endpoint:?}: {detail}")]
    InvalidEndpoint {
        
        endpoint: String,
        
        detail: String,
    },
    
    #[error("entry redirect endpoint must be IPv4 loopback 127.0.0.1:PORT, got {0:?}")]
    Ipv4LoopbackRequired(String),
    
    #[error("entry redirect endpoint port must be non-zero")]
    NonZeroPortRequired,
    
    #[error("entry redirection was requested but the netredirect feature is not compiled")]
    FeatureUnavailable,
    
    #[cfg(feature = "netredirect")]
    #[error("could not load ws2_32.dll for entry redirection: {0}")]
    WinsockLoad(String),
    
    #[cfg(feature = "netredirect")]
    #[error(transparent)]
    Hook(#[from] HookError),

#[cfg(feature = "netredirect")]
    #[error(
        "network hook installation failed ({install_detail}) and rollback also failed ({rollback_detail})"
    )]
    Rollback {
        
        install_detail: String,
        
        rollback_detail: String,
    },
    
    #[cfg(feature = "netredirect")]
    #[error("network hook {0} was already initialized")]
    AlreadyInitialized(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_runtime_configuration_is_side_effect_free() {
        let expected = if cfg!(feature = "netredirect") {
            NetRedirectState::ConfigurationDisabled
        } else {
            NetRedirectState::FeatureDisabled
        };
        assert_eq!(initialize(None).unwrap(), expected);
    }

    #[test]
    fn accepts_only_explicit_ipv4_localhost() {
        assert_eq!(
            parse_replacement("127.0.0.1:2270").unwrap(),
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, 2270)
        );
        assert!(matches!(
            parse_replacement("127.0.0.2:2270"),
            Err(NetRedirectError::Ipv4LoopbackRequired(_))
        ));
        assert!(matches!(
            parse_replacement("[::1]:2270"),
            Err(NetRedirectError::Ipv4LoopbackRequired(_))
        ));
        assert!(matches!(
            parse_replacement("192.0.2.1:2270"),
            Err(NetRedirectError::Ipv4LoopbackRequired(_))
        ));
    }

    #[test]
    fn rejects_zero_port_and_malformed_endpoint() {
        assert!(matches!(
            parse_replacement("127.0.0.1:0"),
            Err(NetRedirectError::NonZeroPortRequired)
        ));
        assert!(matches!(
            parse_replacement("127.0.0.1"),
            Err(NetRedirectError::InvalidEndpoint { .. })
        ));
    }

    #[cfg(feature = "netredirect")]
    #[test]
    fn allowlist_matches_only_the_measured_entry() {
        let replacement = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 2270);
        let rule = RedirectRule {
            original: measured_entry(),
            replacement,
        };
        assert_eq!(rule.replacement_for(measured_entry()), Some(replacement));
        assert_eq!(
            rule.replacement_for(SocketAddrV4::new(MEASURED_ENTRY_IP, 8000)),
            Some(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8000))
        );
        assert_eq!(
            rule.replacement_for(SocketAddrV4::new(MEASURED_ENTRY_IP, 20260)),
            None
        );
        assert_eq!(
            rule.replacement_for(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 1), 2270)),
            None
        );
    }

    #[cfg(feature = "netredirect")]
    #[test]
    fn raw_ipv4_layout_round_trips_network_byte_order() {
        assert_eq!(mem::size_of::<RawSockAddrIn>(), 16);
        let raw = RawSockAddrIn {
            family: AF_INET.0,
            port_network_order: MEASURED_ENTRY_PORT.to_be(),
            address: MEASURED_ENTRY_IP.octets(),
            zero: [0; 8],
        };
        assert_eq!(raw.socket_addr(), Some(measured_entry()));

        let replacement = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 32000);
        let rewritten = raw.with_destination(replacement);
        assert_eq!(rewritten.socket_addr(), Some(replacement));
    }

    #[cfg(feature = "netredirect")]
    #[test]
    fn classifier_observes_nonmatching_20260_without_rewriting_it() {
        let replacement = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 32000);
        let rule = RedirectRule {
            original: measured_entry(),
            replacement,
        };
        let raw = RawSockAddrIn {
            family: AF_INET.0,
            port_network_order: 20260_u16.to_be(),
            address: MEASURED_ENTRY_IP.octets(),
            zero: [0xA5; 8],
        };

        let attempt = classify_ipv4(rule, raw).expect("AF_INET should be observable");
        assert_eq!(
            attempt.original,
            SocketAddrV4::new(MEASURED_ENTRY_IP, 20260)
        );
        assert_eq!(attempt.replacement, None);
        assert_eq!(attempt.raw, raw);
    }

    #[cfg(feature = "netredirect")]
    #[test]
    fn extension_query_helpers_are_length_bounded_and_read_only() {
        let guid = WSAID_CONNECTEX;
        let parsed = read_extension_guid(
            SIO_GET_EXTENSION_FUNCTION_POINTER,
            ptr::from_ref(&guid).cast::<c_void>(),
            u32::try_from(mem::size_of::<GUID>()).unwrap(),
        );
        assert_eq!(parsed, Some(WSAID_CONNECTEX));
        assert_eq!(
            read_extension_guid(
                SIO_GET_EXTENSION_FUNCTION_POINTER,
                ptr::from_ref(&guid).cast::<c_void>(),
                15,
            ),
            None
        );

        let returned = 0x1234usize;
        assert_eq!(
            read_extension_pointer(
                0,
                ptr::from_ref(&returned).cast_mut().cast::<c_void>(),
                u32::try_from(mem::size_of::<usize>()).unwrap(),
            ),
            Some(returned)
        );
        assert_eq!(
            read_extension_pointer(
                -1,
                ptr::from_ref(&returned).cast_mut().cast::<c_void>(),
                u32::try_from(mem::size_of::<usize>()).unwrap(),
            ),
            None
        );
    }
}
