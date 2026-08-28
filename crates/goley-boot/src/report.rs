

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WaitRecord {
    
    pub operation: String,
    
    pub object_name: Option<String>,
    
    pub caller_module: Option<String>,
    
    pub caller_offset: Option<String>,
    
    pub outcome: Option<String>,
    
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WaitSummary {
    
    pub operation: String,
    
    pub object_name: String,
    
    pub caller: String,
    
    pub count: usize,
    
    pub last_outcome: String,
    
    pub potentially_blocking: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TerminationSummary {
    
    pub api: String,
    
    pub status: String,
    
    pub caller: String,
    
    pub count: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CaptureReport {
    
    pub parsed_records: usize,
    
    pub ignored_lines: usize,
    
    pub summaries: Vec<WaitSummary>,
    
    pub termination_summaries: Vec<TerminationSummary>,
}

impl CaptureReport {
    
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut output = String::from(
            "# Goley startup wait-object report\n\n\
             This report contains observations only; it does not select or hard-code a GameGuard object.\n\n",
        );
        output.push_str(&format!(
            "- Parsed records: {}\n- Ignored lines: {}\n- Unique object/wait call sites: {}\n- Termination call sites: {}\n\n",
            self.parsed_records,
            self.ignored_lines,
            self.summaries.len(),
            self.termination_summaries.len()
        ));
        output.push_str(
            "| Blocking candidate | Operation | Object | Caller | Count | Last outcome |\n",
        );
        output.push_str("|---|---|---|---|---:|---|\n");
        for summary in &self.summaries {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                if summary.potentially_blocking {
                    "yes"
                } else {
                    "no"
                },
                escape_cell(&summary.operation),
                escape_cell(&summary.object_name),
                escape_cell(&summary.caller),
                summary.count,
                escape_cell(&summary.last_outcome)
            ));
        }
        if self.summaries.is_empty() {
            output.push_str("| no | — | — | — | 0 | no observations |\n");
        }
        output.push_str("\n## Termination observations\n\n");
        output.push_str("| API | Status | Caller | Count |\n");
        output.push_str("|---|---|---|---:|\n");
        for summary in &self.termination_summaries {
            output.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                escape_cell(&summary.api),
                escape_cell(&summary.status),
                escape_cell(&summary.caller),
                summary.count
            ));
        }
        if self.termination_summaries.is_empty() {
            output.push_str("| — | — | — | 0 |\n");
        }
        output
    }
}

#[must_use]
pub fn parse_capture_text(input: &str) -> CaptureReport {
    let mut wait_records = Vec::new();
    let mut termination_records = Vec::new();
    let mut ignored_lines = 0;
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let parsed = parse_line(line);
        if parsed.is_empty() {
            ignored_lines += 1;
        } else {
            for observation in parsed {
                match observation {
                    Observation::Wait(record) => wait_records.push(record),
                    Observation::Termination(record) => termination_records.push(record),
                }
            }
        }
    }
    aggregate(wait_records, termination_records, ignored_lines)
}

pub fn parse_capture_log(path: &Path) -> Result<CaptureReport> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("failed to read shim capture log {}", path.display()))?;
    Ok(parse_capture_text(&input))
}

pub fn write_report(path: &Path, report: &CaptureReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report directory {}", parent.display()))?;
    }
    fs::write(path, report.to_markdown())
        .with_context(|| format!("failed to write report {}", path.display()))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum WaitPhase {
    Enter,
    Return,
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct PhasedWaitRecord {
    record: WaitRecord,
    phase: WaitPhase,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct TerminationRecord {
    api: String,
    status: String,
    caller_module: Option<String>,
    caller_offset: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum Observation {
    Wait(PhasedWaitRecord),
    Termination(TerminationRecord),
}

fn parse_line(line: &str) -> Vec<Observation> {
    if let Ok(value) = serde_json::from_str::<Value>(line) {
        let records = parse_json(&value);
        if !records.is_empty() {
            return records;
        }
    }
    parse_pipe_text(line)
        .or_else(|| parse_key_value_text(line))
        .map(Observation::Wait)
        .into_iter()
        .collect()
}

fn parse_json(value: &Value) -> Vec<Observation> {
    if string_field(value, &["event_type"]).as_deref() == Some("self_termination_suppressed") {
        return parse_termination_json(value)
            .map(Observation::Termination)
            .into_iter()
            .collect();
    }

    let Some(operation) = string_field(value, &["api", "function", "op", "operation"]) else {
        return Vec::new();
    };
    if !is_observed_operation(&operation) {
        return Vec::new();
    }
    let phase = string_field(value, &["operation", "phase"])
        .as_deref()
        .map(wait_phase)
        .unwrap_or(WaitPhase::Unknown);
    let names = object_names(value);
    let caller_module = string_field(value, &["caller_module", "module"]);
    let caller_offset = offset_field(value, &["caller_offset", "caller_rva", "offset", "rva"]);
    let outcome = string_field(value, &["wait_result", "outcome", "result", "status"]);
    let timeout_ms = integer_field(value, &["timeout_ms", "timeout"]);
    names
        .into_iter()
        .map(|object_name| {
            Observation::Wait(PhasedWaitRecord {
                record: WaitRecord {
                    operation: operation.clone(),
                    object_name,
                    caller_module: caller_module.clone(),
                    caller_offset: caller_offset.clone(),
                    outcome: outcome.clone(),
                    timeout_ms,
                },
                phase,
            })
        })
        .collect()
}

fn parse_termination_json(value: &Value) -> Option<TerminationRecord> {
    Some(TerminationRecord {
        api: string_field(value, &["api", "operation", "function"])?,
        status: string_field(value, &["status", "exit_code", "code"])
            .unwrap_or_else(|| "<unknown>".into()),
        caller_module: string_field(value, &["caller_module", "module"]),
        caller_offset: offset_field(value, &["caller_offset", "caller_rva", "offset", "rva"]),
    })
}

fn wait_phase(value: &str) -> WaitPhase {
    match value.to_ascii_lowercase().as_str() {
        "wait_enter" | "enter" => WaitPhase::Enter,
        "wait_return" | "return" => WaitPhase::Return,
        _ => WaitPhase::Unknown,
    }
}

fn object_names(value: &Value) -> Vec<Option<String>> {
    if let Some(name) = string_field(value, &["object_name", "object", "name"]) {
        return vec![Some(name)];
    }
    match value.get("object_names") {
        Some(Value::Array(names)) => {
            let names = names
                .iter()
                .filter_map(|name| name.as_str().map(ToOwned::to_owned))
                .map(Some)
                .collect::<Vec<_>>();
            if names.is_empty() { vec![None] } else { names }
        }
        Some(Value::String(names)) => {
            let parsed = serde_json::from_str::<Vec<String>>(names)
                .unwrap_or_else(|_| parse_debug_list(names));
            if parsed.is_empty() {
                vec![None]
            } else {
                parsed.into_iter().map(Some).collect()
            }
        }
        _ => vec![None],
    }
}

fn parse_debug_list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches(['"', '\'']))
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value.get(*name).and_then(|field| match field {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        })
    })
}

fn integer_field(value: &Value, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        value.get(*name).and_then(|field| {
            field.as_u64().or_else(|| {
                field
                    .as_str()
                    .and_then(|text| text.trim().parse::<u64>().ok())
            })
        })
    })
}

fn offset_field(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value.get(*name).and_then(|field| match field {
            Value::Number(number) => number.as_u64().map(|offset| format!("0x{offset:x}")),
            Value::String(text) => Some(text.clone()),
            _ => None,
        })
    })
}

fn parse_pipe_text(line: &str) -> Option<PhasedWaitRecord> {
    let fields = line.split('|').map(str::trim).collect::<Vec<_>>();
    if fields.len() < 2 {
        return None;
    }
    let operation_index = fields
        .iter()
        .position(|field| is_observed_operation(field))?;
    let operation = fields[operation_index].to_owned();
    let tail = &fields[operation_index + 1..];
    Some(PhasedWaitRecord {
        record: WaitRecord {
            operation,
            object_name: nonempty(tail.first().copied()),
            caller_module: nonempty(tail.get(1).copied()),
            caller_offset: nonempty(tail.get(2).copied()),
            outcome: nonempty(tail.get(3).copied()),
            timeout_ms: tail.get(4).and_then(|value| value.parse().ok()),
        },
        phase: WaitPhase::Unknown,
    })
}

fn parse_key_value_text(line: &str) -> Option<PhasedWaitRecord> {
    let mut fields = BTreeMap::new();
    for token in line.split_whitespace() {
        if let Some((key, value)) = token.split_once('=') {
            fields.insert(
                key.trim().to_ascii_lowercase(),
                value.trim_matches(['"', '\'']).to_owned(),
            );
        }
    }
    let operation = ["api", "function", "op", "operation"]
        .iter()
        .find_map(|key| fields.get(*key).cloned())?;
    if !is_observed_operation(&operation) {
        return None;
    }
    let phase = first_map_value(&fields, &["operation", "phase"])
        .as_deref()
        .map(wait_phase)
        .unwrap_or(WaitPhase::Unknown);
    Some(PhasedWaitRecord {
        record: WaitRecord {
            operation,
            object_name: first_map_value(&fields, &["object_name", "object", "name"]),
            caller_module: first_map_value(&fields, &["caller_module", "module"]),
            caller_offset: first_map_value(&fields, &["caller_offset", "offset", "rva"]),
            outcome: first_map_value(&fields, &["wait_result", "outcome", "result", "status"]),
            timeout_ms: first_map_value(&fields, &["timeout_ms", "timeout"])
                .and_then(|value| value.parse().ok()),
        },
        phase,
    })
}

fn first_map_value(fields: &BTreeMap<String, String>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| fields.get(*name).cloned())
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.is_empty() && *value != "-")
        .map(ToOwned::to_owned)
}

fn is_observed_operation(operation: &str) -> bool {
    let normalized = operation.to_ascii_lowercase();
    [
        "createevent",
        "openevent",
        "createmutex",
        "openmutex",
        "waitforsingleobject",
        "waitformultipleobjects",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

struct WaitGroup {
    summary: WaitSummary,
    unmatched_pending: usize,
    saw_timeout: bool,
    saw_unpaired_blocker: bool,
}

fn aggregate(
    records: Vec<PhasedWaitRecord>,
    termination_records: Vec<TerminationRecord>,
    ignored_lines: usize,
) -> CaptureReport {
    let parsed_records = records.len() + termination_records.len();
    let mut grouped: BTreeMap<(String, String, String), WaitGroup> = BTreeMap::new();
    for phased_record in records {
        let PhasedWaitRecord { record, phase } = phased_record;
        let object_name = record.object_name.unwrap_or_else(|| "<unnamed>".into());
        let caller = format_caller(record.caller_module, record.caller_offset);
        let outcome = record.outcome.unwrap_or_else(|| "<unknown>".into());
        let is_wait = record.operation.to_ascii_lowercase().contains("waitfor");
        let timed_out = is_timeout_outcome(&outcome);
        let pending = is_pending_outcome(&outcome);
        let infinite_timeout = record
            .timeout_ms
            .is_some_and(|timeout| timeout == u64::from(u32::MAX));
        let key = (
            record.operation.clone(),
            object_name.clone(),
            caller.clone(),
        );
        let group = grouped.entry(key).or_insert_with(|| WaitGroup {
            summary: WaitSummary {
                operation: record.operation,
                object_name,
                caller,
                count: 0,
                last_outcome: outcome.clone(),
                potentially_blocking: false,
            },
            unmatched_pending: 0,
            saw_timeout: false,
            saw_unpaired_blocker: false,
        });
        group.summary.count += 1;
        group.summary.last_outcome = outcome;

        if is_wait {
            group.saw_timeout |= timed_out;
            match phase {
                WaitPhase::Enter => {
                    if pending || infinite_timeout {
                        group.unmatched_pending += 1;
                    }
                }
                WaitPhase::Return => {
                    group.unmatched_pending = group.unmatched_pending.saturating_sub(1);
                    group.saw_unpaired_blocker |= pending;
                }
                WaitPhase::Unknown => {
                    if pending || infinite_timeout {
                        group.saw_unpaired_blocker = true;
                    }
                }
            }
        }
    }

    let mut summaries = grouped
        .into_values()
        .map(|mut group| {
            group.summary.potentially_blocking =
                group.saw_timeout || group.saw_unpaired_blocker || group.unmatched_pending != 0;
            group.summary
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .potentially_blocking
            .cmp(&left.potentially_blocking)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.object_name.cmp(&right.object_name))
            .then_with(|| left.operation.cmp(&right.operation))
            .then_with(|| left.caller.cmp(&right.caller))
    });

    let mut termination_grouped: BTreeMap<(String, String, String), TerminationSummary> =
        BTreeMap::new();
    for record in termination_records {
        let caller = format_caller(record.caller_module, record.caller_offset);
        let key = (record.api.clone(), record.status.clone(), caller.clone());
        termination_grouped
            .entry(key)
            .or_insert_with(|| TerminationSummary {
                api: record.api,
                status: record.status,
                caller,
                count: 0,
            })
            .count += 1;
    }
    let mut termination_summaries = termination_grouped.into_values().collect::<Vec<_>>();
    termination_summaries.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.api.cmp(&right.api))
            .then_with(|| left.status.cmp(&right.status))
            .then_with(|| left.caller.cmp(&right.caller))
    });

    CaptureReport {
        parsed_records,
        ignored_lines,
        summaries,
        termination_summaries,
    }
}

fn format_caller(module: Option<String>, offset: Option<String>) -> String {
    match (module, offset) {
        (Some(module), Some(offset)) => format!("{module}+{offset}"),
        (Some(module), None) => module,
        (None, Some(offset)) => format!("<unknown>+{offset}"),
        (None, None) => "<unknown>".into(),
    }
}

fn is_timeout_outcome(outcome: &str) -> bool {
    let normalized = outcome.trim().to_ascii_lowercase();
    normalized.contains("wait_timeout")
        || normalized == "timeout"
        || normalized.parse::<u64>().is_ok_and(|status| status == 258)
        || normalized
            .strip_prefix("0x")
            .and_then(|status| u64::from_str_radix(status, 16).ok())
            .is_some_and(|status| status == 258)
}

fn is_pending_outcome(outcome: &str) -> bool {
    let normalized = outcome.to_ascii_lowercase();
    normalized.contains("pending") || normalized.contains("still")
}

fn escape_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_owned()
}
