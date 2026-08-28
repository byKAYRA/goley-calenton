#![cfg(feature = "netredirect")]

use std::{
    fs,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    time::Duration,
};

use goley_shim::{logging, netredirect};

#[test]
fn allowlisted_connect_reaches_loopback_and_logs_both_destinations() {
    let directory = tempfile::tempdir().expect("temporary log directory should be available");
    let log_path = directory.path().join("netredirect.jsonl");
    let _logging = logging::init(&log_path, "info").expect("JSONL logging should initialize");

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("ephemeral loopback listener should bind");
    let replacement = listener
        .local_addr()
        .expect("loopback listener should have an address");
    let state = netredirect::initialize(Some(&replacement.to_string()))
        .expect("allowlisted hooks should install");
    assert!(matches!(
        state,
        netredirect::NetRedirectState::Installed { .. }
    ));

    let measured_legacy = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(213, 74, 179, 12), 2270));
    let _client = TcpStream::connect_timeout(&measured_legacy, Duration::from_secs(2))
        .expect("measured legacy destination should be redirected to loopback");
    let (_accepted, peer) = listener
        .accept()
        .expect("loopback listener should receive the redirected connection");
    assert!(peer.ip().is_loopback());

    let passthrough_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("second ephemeral loopback listener should bind");
    let passthrough_destination = passthrough_listener
        .local_addr()
        .expect("pass-through listener should have an address");
    let _passthrough_client = TcpStream::connect(passthrough_destination)
        .expect("nonmatching IPv4 destination should pass through unchanged");
    let (_passthrough_accepted, passthrough_peer) = passthrough_listener
        .accept()
        .expect("original nonmatching listener should receive the connection");
    assert!(passthrough_peer.ip().is_loopback());

    let jsonl = fs::read_to_string(&log_path).expect("redirect event should be flushed");
    let events = jsonl
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid JSONL record"))
        .collect::<Vec<_>>();
    let event = events
        .iter()
        .find(|value| value["event_type"] == "network_connect_redirect")
        .expect("redirect event should be present");
    assert_eq!(event["original_destination"], "213.74.179.12:2270");
    assert_eq!(event["redirected_destination"], replacement.to_string());
    assert_eq!(event["matched"], true);
    assert!(matches!(
        event["api"].as_str(),
        Some("connect" | "WSAConnect")
    ));

    let matched_attempt = events
        .iter()
        .find(|value| {
            value["event_type"] == "network_connect_attempt"
                && value["original_destination"] == "213.74.179.12:2270"
        })
        .expect("matched attempt event should be present");
    assert_eq!(matched_attempt["matched"], true);

    let passthrough_attempt = events
        .iter()
        .find(|value| {
            value["event_type"] == "network_connect_attempt"
                && value["original_destination"] == passthrough_destination.to_string()
        })
        .expect("nonmatching attempt event should be present");
    assert_eq!(passthrough_attempt["matched"], false);
    assert!(passthrough_attempt.get("redirected_destination").is_none());
}
