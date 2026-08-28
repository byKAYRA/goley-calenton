

use clap::Parser;
use goley_boot::config::{ShimConfig, ShimMode, UnpackConfig};

#[test]
fn shim_configuration_round_trips_as_json() {
    let config = ShimConfig {
        mode: ShimMode::Run,
        region: Some("TRAuth".into()),
        entry: Some("127.0.0.1:2270".into()),
        loaded_event: Some(r"Local\GoleyBoot-test-loaded".into()),
        ready_event: Some(r"Local\GoleyBoot-test-ready".into()),
        gameguard_ready_event: Some(r"Local\ObservedGameGuardReady".into()),
        patches_path: Some(std::path::PathBuf::from("patches.toml")),
        log_path: std::env::temp_dir().join("goley-boot-test.jsonl"),
        verbosity: "goley_shim=debug,info".into(),
        unpack: UnpackConfig {
            oep_rva: None,
            poll_interval_ms: 5,
            stable_samples: 4,
            timeout_ms: 30_000,
            post_ready_delay_ms: 8,
        },
        post_unpack_gate: None,
    };

    let encoded = serde_json::to_string(&config).expect("configuration should serialize");
    let decoded: ShimConfig =
        serde_json::from_str(&encoded).expect("configuration should deserialize");
    assert_eq!(decoded, config);
}

#[test]
fn capture_log_becomes_a_sorted_clean_report() {
    let raw = concat!(
        r#"{"api":"CreateEventW","name":"GG_READY","module":"Goley.exe","rva":"0x10"}"#,
        "\n",
        r#"{"api":"WaitForSingleObject","name":"GG_READY","module":"Goley.exe","rva":"0x20","result":"WAIT_TIMEOUT","timeout_ms":4294967295}"#,
        "\nnoise\n"
    );
    let report = goley_boot::report::parse_capture_text(raw);
    assert_eq!(report.parsed_records, 2);
    assert_eq!(report.ignored_lines, 1);
    assert!(report.summaries[0].potentially_blocking);
    assert!(report.to_markdown().contains("GG_READY"));
}

#[test]
fn documented_run_command_parses_without_touching_the_client() {
    let cli = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "run",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
        "--region",
        "TRAuth",
        "--runparam-key",
        "TOKEN",
        "--entry",
        "127.0.0.1:2270",
        "--late-inject-ms",
        "8",
        "--timeout",
        "30",
        "--pre-resume-gate",
        r"C:\Temp\goley.resume",
        "--pre-resume-gate-timeout",
        "180",
    ])
    .expect("documented command should parse");

    let goley_boot::cli::BootCommand::Run(args) = cli.command else {
        panic!("expected run command");
    };
    assert_eq!(
        args.pre_resume.pre_resume_gate.as_deref(),
        Some(std::path::Path::new(r"C:\Temp\goley.resume"))
    );
    assert_eq!(args.pre_resume.pre_resume_gate_timeout, 180);
    assert_eq!(args.runparam_key.as_deref(), Some("TOKEN"));
}

#[test]
fn capture_command_accepts_the_same_pre_resume_gate() {
    let cli = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "capture-waits",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
        "--pre-resume-gate",
        r"C:\Temp\goley-capture.resume",
        "--runparam-key",
        "TOKEN",
    ])
    .expect("capture gate should parse");

    let goley_boot::cli::BootCommand::CaptureWaits(args) = cli.command else {
        panic!("expected capture-waits command");
    };
    assert_eq!(
        args.pre_resume.pre_resume_gate.as_deref(),
        Some(std::path::Path::new(r"C:\Temp\goley-capture.resume"))
    );
    assert_eq!(args.pre_resume.pre_resume_gate_timeout, 120);
    assert_eq!(args.runparam_key.as_deref(), Some("TOKEN"));
}

#[test]
fn run_and_capture_default_to_the_legacy_runparam() {
    let run = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "run",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
        "--region",
        "TRAuth",
    ])
    .expect("run command should parse");
    let goley_boot::cli::BootCommand::Run(run) = run.command else {
        panic!("expected run command");
    };
    assert_eq!(run.runparam_key, None);

    let capture = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "capture-waits",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
    ])
    .expect("capture command should parse");
    let goley_boot::cli::BootCommand::CaptureWaits(capture) = capture.command else {
        panic!("expected capture-waits command");
    };
    assert_eq!(capture.runparam_key, None);
}

#[test]
fn runparam_key_rejects_non_token_values() {
    for invalid in [
        "",
        "two words",
        "tab\tkey",
        "quote\"key",
        "quote'key",
        "nul\0key",
    ] {
        let error = goley_boot::Cli::try_parse_from([
            "goley-boot",
            "run",
            "--client",
            r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
            "--region",
            "TRAuth",
            "--runparam-key",
            invalid,
        ])
        .expect_err("invalid runparam key must be rejected");
        assert!(
            error.to_string().contains("runparam key"),
            "unexpected parse error for {invalid:?}: {error}"
        );
    }
}

#[test]
fn dump_unpacked_does_not_accept_runparam_key() {
    let error = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "dump-unpacked",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
        "--out",
        r"C:\Temp\goley-unpacked.dump",
        "--runparam-key",
        "TOKEN",
    ])
    .expect_err("dump-unpacked must keep its existing command-line contract");
    assert!(error.to_string().contains("--runparam-key"));
}

#[test]
fn run_command_accepts_a_measured_post_unpack_gate() {
    let cli = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "run",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
        "--region",
        "TRAuth",
        "--post-unpack-gate",
        r"C:\Temp\goley-post-unpack.release",
        "--post-unpack-gate-rva",
        "0x1234",
        "--post-unpack-gate-timeout",
        "180",
    ])
    .expect("post-unpack gate should parse");

    let goley_boot::cli::BootCommand::Run(args) = cli.command else {
        panic!("expected run command");
    };
    assert_eq!(args.post_unpack.post_unpack_gate_rva, Some(0x1234));
    assert_eq!(args.post_unpack.post_unpack_gate_timeout, 180);
}

#[test]
fn documented_dump_command_parses_without_touching_the_client() {
    let cli = goley_boot::Cli::try_parse_from([
        "goley-boot",
        "dump-unpacked",
        "--client",
        r"C:\Joygame\Goley\BinaryTr\BinaryTr.bin",
        "--out",
        r"C:\Temp\goley-unpacked.dump",
    ])
    .expect("documented dump command should parse");

    assert!(matches!(
        cli.command,
        goley_boot::cli::BootCommand::DumpUnpacked(_)
    ));
}

#[test]
fn shim_multi_object_wait_expands_and_reads_numeric_wait_result() {
    let raw = r#"{"event_type":"kernel_wait","operation":"wait","api":"WaitForMultipleObjects","object_names":["GG_READY","PATCH_READY"],"timeout_ms":30000,"wait_result":258,"caller_module":"BinaryTr.bin","caller_offset":4660}"#;
    let report = goley_boot::report::parse_capture_text(raw);

    assert_eq!(report.parsed_records, 2);
    assert_eq!(report.summaries.len(), 2);
    assert!(
        report
            .summaries
            .iter()
            .all(|item| item.potentially_blocking)
    );
    assert!(
        report
            .summaries
            .iter()
            .all(|item| item.caller == "BinaryTr.bin+0x1234")
    );
}

#[test]
fn completed_wait_enter_pair_is_not_reported_as_blocking() {
    let raw = concat!(
        r#"{"event_type":"kernel_wait","operation":"wait_enter","outcome":"pending","api":"WaitForSingleObject","object_names":["GG_READY"],"timeout_ms":4294967295,"wait_result":null,"caller_module":"BinaryTr.bin","caller_offset":4660}"#,
        "\n",
        r#"{"event_type":"kernel_wait","operation":"wait_return","outcome":"returned","api":"WaitForSingleObject","object_names":["GG_READY"],"timeout_ms":4294967295,"wait_result":0,"caller_module":"BinaryTr.bin","caller_offset":4660}"#
    );

    let report = goley_boot::report::parse_capture_text(raw);

    assert_eq!(report.parsed_records, 2);
    assert_eq!(report.summaries.len(), 1);
    assert_eq!(report.summaries[0].count, 2);
    assert_eq!(report.summaries[0].last_outcome, "0");
    assert!(!report.summaries[0].potentially_blocking);
}

#[test]
fn unmatched_pending_and_wait_timeout_remain_blocking_candidates() {
    let raw = concat!(
        r#"{"event_type":"kernel_wait","operation":"wait_enter","outcome":"pending","api":"WaitForSingleObject","object_names":["UNMATCHED"],"timeout_ms":4294967295,"caller_module":"BinaryTr.bin","caller_offset":16}"#,
        "\n",
        r#"{"event_type":"kernel_wait","operation":"wait_enter","outcome":"pending","api":"WaitForSingleObject","object_names":["TIMED_OUT"],"timeout_ms":50,"caller_module":"BinaryTr.bin","caller_offset":32}"#,
        "\n",
        r#"{"event_type":"kernel_wait","operation":"wait_return","outcome":"returned","api":"WaitForSingleObject","object_names":["TIMED_OUT"],"timeout_ms":50,"wait_result":258,"caller_module":"BinaryTr.bin","caller_offset":32}"#
    );

    let report = goley_boot::report::parse_capture_text(raw);

    assert_eq!(report.parsed_records, 3);
    assert_eq!(report.summaries.len(), 2);
    assert!(
        report
            .summaries
            .iter()
            .all(|summary| summary.potentially_blocking)
    );
}

#[test]
fn suppressed_termination_records_have_a_separate_aggregate_table() {
    let raw = concat!(
        r#"{"event_type":"self_termination_suppressed","api":"ExitProcess","status":7,"caller_module":"BinaryTr.bin","caller_offset":64}"#,
        "\n",
        r#"{"event_type":"self_termination_suppressed","api":"ExitProcess","status":7,"caller_module":"BinaryTr.bin","caller_offset":64}"#,
        "\n",
        r#"{"event_type":"self_termination_suppressed","api":"NtTerminateProcess","status":-1073741819,"caller_module":"ntdll.dll","caller_offset":128}"#
    );

    let report = goley_boot::report::parse_capture_text(raw);

    assert_eq!(report.parsed_records, 3);
    assert_eq!(report.ignored_lines, 0);
    assert!(report.summaries.is_empty());
    assert_eq!(report.termination_summaries.len(), 2);
    assert_eq!(report.termination_summaries[0].api, "ExitProcess");
    assert_eq!(report.termination_summaries[0].status, "7");
    assert_eq!(report.termination_summaries[0].caller, "BinaryTr.bin+0x40");
    assert_eq!(report.termination_summaries[0].count, 2);

    let markdown = report.to_markdown();
    assert!(markdown.contains("## Termination observations"));
    assert!(markdown.contains("| API | Status | Caller | Count |"));
    assert!(markdown.contains("| ExitProcess | 7 | BinaryTr.bin+0x40 | 2 |"));
}
