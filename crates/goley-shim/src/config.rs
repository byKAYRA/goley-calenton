

use std::{env, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONFIG_ENV: &str = "GOLEY_SHIM_CONFIG";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShimMode {

#[default]
    Run,
    
    CaptureWaits,
    
    DumpUnpacked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UnpackConfig {

pub oep_rva: Option<u32>,
    
    pub poll_interval_ms: u64,
    
    pub stable_samples: u32,
    
    pub timeout_ms: u64,

pub post_ready_delay_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PostUnpackGateConfig {
    
    pub target_rva: u32,
    
    pub arrived_event: String,
    
    pub release_event: String,
    
    pub timeout_ms: u64,
}

impl Default for UnpackConfig {
    fn default() -> Self {
        Self {
            oep_rva: None,
            poll_interval_ms: 5,
            stable_samples: 3,
            timeout_ms: 30_000,
            post_ready_delay_ms: 8,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShimConfig {
    
    pub mode: ShimMode,
    
    pub region: Option<String>,

pub entry: Option<String>,
    
    pub verbosity: String,
    
    pub log_path: PathBuf,
    
    pub loaded_event: Option<String>,
    
    pub ready_event: Option<String>,

pub gameguard_ready_event: Option<String>,
    
    pub patches_path: Option<PathBuf>,
    
    pub unpack: UnpackConfig,
    
    pub post_unpack_gate: Option<PostUnpackGateConfig>,
}

impl Default for ShimConfig {
    fn default() -> Self {
        let log_path = env::temp_dir().join(format!("goley-shim-{}.jsonl", std::process::id()));
        Self {
            mode: ShimMode::Run,
            region: None,
            entry: None,
            verbosity: "info".to_owned(),
            log_path,
            loaded_event: None,
            ready_event: None,
            gameguard_ready_event: None,
            patches_path: None,
            unpack: UnpackConfig::default(),
            post_unpack_gate: None,
        }
    }
}

impl ShimConfig {
    
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

pub fn from_env() -> Result<Self, ConfigError> {
        match env::var(CONFIG_ENV) {
            Ok(json) => Self::from_json(&json),
            Err(env::VarError::NotPresent) => Ok(Self::default()),
            Err(error) => Err(ConfigError::Environment(error)),
        }
    }

pub fn validate(&self) -> Result<(), ConfigError> {
        if self.entry.is_some() && self.mode != ShimMode::Run {
            return Err(ConfigError::Invalid(
                "entry redirection is valid only in run mode".to_owned(),
            ));
        }
        if let Some(entry) = &self.entry {
            crate::netredirect::validate_entry(entry)
                .map_err(|error| ConfigError::Invalid(error.to_string()))?;
        }
        if self.unpack.poll_interval_ms == 0 {
            return Err(ConfigError::Invalid(
                "unpack.poll_interval_ms must be greater than zero".to_owned(),
            ));
        }
        if self.unpack.stable_samples == 0 {
            return Err(ConfigError::Invalid(
                "unpack.stable_samples must be greater than zero".to_owned(),
            ));
        }
        if self.unpack.timeout_ms == 0 {
            return Err(ConfigError::Invalid(
                "unpack.timeout_ms must be greater than zero".to_owned(),
            ));
        }
        if self.loaded_event.is_some() && self.loaded_event == self.ready_event {
            return Err(ConfigError::Invalid(
                "loaded_event and ready_event must be distinct".to_owned(),
            ));
        }
        if let Some(gate) = &self.post_unpack_gate {
            if gate.target_rva == 0 {
                return Err(ConfigError::Invalid(
                    "post_unpack_gate.target_rva must not be zero".to_owned(),
                ));
            }
            if gate.timeout_ms == 0 {
                return Err(ConfigError::Invalid(
                    "post_unpack_gate.timeout_ms must be greater than zero".to_owned(),
                ));
            }
            if gate.arrived_event.is_empty() || gate.release_event.is_empty() {
                return Err(ConfigError::Invalid(
                    "post-unpack gate event names must not be empty".to_owned(),
                ));
            }
            if gate.arrived_event == gate.release_event {
                return Err(ConfigError::Invalid(
                    "post-unpack arrived_event and release_event must be distinct".to_owned(),
                ));
            }
            let handshake_events = [self.loaded_event.as_deref(), self.ready_event.as_deref()];
            if handshake_events
                .into_iter()
                .flatten()
                .any(|name| name == gate.arrived_event || name == gate.release_event)
            {
                return Err(ConfigError::Invalid(
                    "post-unpack gate events must be distinct from handshake events".to_owned(),
                ));
            }
        }
        if let Some(filter) = self.verbosity.strip_prefix(' ') {
            let _ = filter;
            return Err(ConfigError::Invalid(
                "verbosity must not start with whitespace".to_owned(),
            ));
        }
        
        tracing_subscriber::EnvFilter::from_str(&self.verbosity)
            .map_err(|error| ConfigError::Invalid(format!("invalid verbosity filter: {error}")))?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    
    #[error("configuration environment error: {0}")]
    Environment(#[from] env::VarError),
    
    #[error("configuration JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("invalid shim configuration: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_mode_uses_kebab_case() {
        let config = ShimConfig::from_json(r#"{"mode":"capture-waits"}"#).unwrap();
        assert_eq!(config.mode, ShimMode::CaptureWaits);
    }

    #[test]
    fn dump_mode_uses_kebab_case() {
        let config = ShimConfig::from_json(r#"{"mode":"dump-unpacked"}"#).unwrap();
        assert_eq!(config.mode, ShimMode::DumpUnpacked);
    }

    #[test]
    fn rejects_busy_polling() {
        let config = ShimConfig {
            unpack: UnpackConfig {
                poll_interval_ms: 0,
                ..UnpackConfig::default()
            },
            ..ShimConfig::default()
        };
        assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn entry_requires_exact_ipv4_localhost() {
        let config = ShimConfig {
            entry: Some("192.0.2.1:2270".to_owned()),
            ..ShimConfig::default()
        };
        assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn entry_is_rejected_outside_run_mode() {
        let config = ShimConfig {
            mode: ShimMode::CaptureWaits,
            entry: Some("127.0.0.1:2270".to_owned()),
            ..ShimConfig::default()
        };
        assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn loopback_entry_requires_compiled_feature() {
        let config = ShimConfig {
            entry: Some("127.0.0.1:2270".to_owned()),
            ..ShimConfig::default()
        };
        if cfg!(feature = "netredirect") {
            config
                .validate()
                .expect("feature build should accept entry");
        } else {
            assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
        }
    }

    #[test]
    fn parses_boot_post_unpack_gate_contract() {
        let config = ShimConfig::from_json(
            r#"{
                "post_unpack_gate": {
                    "target_rva": 4660,
                    "arrived_event": "Local\\arrived",
                    "release_event": "Local\\release",
                    "timeout_ms": 45000
                }
            }"#,
        )
        .expect("boot contract should parse");
        let gate = config
            .post_unpack_gate
            .expect("post-unpack gate should be present");
        assert_eq!(gate.target_rva, 0x1234);
        assert_eq!(gate.timeout_ms, 45_000);
    }

    #[test]
    fn rejects_colliding_post_unpack_events() {
        let config = ShimConfig {
            loaded_event: Some("Local\\same".to_owned()),
            post_unpack_gate: Some(PostUnpackGateConfig {
                target_rva: 1,
                arrived_event: "Local\\same".to_owned(),
                release_event: "Local\\release".to_owned(),
                timeout_ms: 1,
            }),
            ..ShimConfig::default()
        };
        assert!(matches!(config.validate(), Err(ConfigError::Invalid(_))));
    }
}
