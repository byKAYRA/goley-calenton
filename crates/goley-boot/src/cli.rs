

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "goley-boot",
    about = "Launch the unmodified Goley client with the clean-room runtime shim"
)]
pub struct Cli {
    
    #[command(subcommand)]
    pub command: BootCommand,
}

#[derive(Debug, Subcommand)]
pub enum BootCommand {
    
    Run(RunArgs),
    
    CaptureWaits(CaptureWaitsArgs),
    
    DumpUnpacked(DumpUnpackedArgs),
}

#[derive(Debug, Clone, Args)]
pub struct LaunchArgs {
    
    #[arg(long, value_name = "PATH")]
    pub client: PathBuf,

#[arg(long, value_name = "PATH")]
    pub shim: Option<PathBuf>,

#[arg(long, value_name = "PATH")]
    pub patches: Option<PathBuf>,

#[arg(long, default_value_t = 8, value_name = "MILLISECONDS")]
    pub late_inject_ms: u64,

#[arg(long, value_parser = parse_u32, value_name = "RVA")]
    pub oep_rva: Option<u32>,

#[arg(long, default_value_t = 5, value_name = "MILLISECONDS")]
    pub unpack_poll_ms: u64,

#[arg(long, default_value_t = 4, value_name = "COUNT")]
    pub unpack_stable_samples: u32,

#[arg(long, default_value_t = 30, value_name = "SECONDS")]
    pub timeout: u64,

#[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Clone, Args)]
pub struct PreResumeGateArgs {
    
    #[arg(long, value_name = "PATH")]
    pub pre_resume_gate: Option<PathBuf>,

#[arg(long, default_value_t = 120, value_name = "SECONDS")]
    pub pre_resume_gate_timeout: u64,
}

#[derive(Debug, Clone, Args)]
pub struct PostUnpackGateArgs {
    
    #[arg(long, value_name = "PATH")]
    pub post_unpack_gate: Option<PathBuf>,

#[arg(long, value_parser = parse_u32, value_name = "RVA")]
    pub post_unpack_gate_rva: Option<u32>,

#[arg(long, default_value_t = 120, value_name = "SECONDS")]
    pub post_unpack_gate_timeout: u64,
}

#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    
    #[command(flatten)]
    pub launch: LaunchArgs,

#[command(flatten)]
    pub pre_resume: PreResumeGateArgs,

#[command(flatten)]
    pub post_unpack: PostUnpackGateArgs,

#[arg(long, value_name = "REGION")]
    pub region: String,

#[arg(long, value_parser = parse_runparam_key, value_name = "TOKEN")]
    pub runparam_key: Option<String>,

#[arg(long, value_name = "IP:PORT")]
    pub entry: Option<SocketAddr>,

#[arg(long, value_name = "NAME")]
    pub gameguard_ready_event: Option<String>,

#[arg(long)]
    pub detach: bool,
}

fn parse_u32(value: &str) -> Result<u32, String> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).map_err(|error| error.to_string())
    } else {
        value.parse::<u32>().map_err(|error| error.to_string())
    }
}

fn parse_runparam_key(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err("runparam key must not be empty".to_owned());
    }
    if value.contains('\0') {
        return Err("runparam key contains an embedded NUL".to_owned());
    }
    if value.contains(['\'', '"']) {
        return Err("runparam key must not contain quote characters".to_owned());
    }
    if value.chars().any(char::is_whitespace) {
        return Err("runparam key must not contain whitespace".to_owned());
    }
    Ok(value.to_owned())
}

#[derive(Debug, Clone, Args)]
pub struct CaptureWaitsArgs {
    
    #[command(flatten)]
    pub launch: LaunchArgs,

#[command(flatten)]
    pub pre_resume: PreResumeGateArgs,

#[command(flatten)]
    pub post_unpack: PostUnpackGateArgs,

#[arg(long, default_value = "TRAuth", value_name = "REGION")]
    pub region: String,

#[arg(long, value_parser = parse_runparam_key, value_name = "TOKEN")]
    pub runparam_key: Option<String>,

#[arg(long, value_name = "PATH")]
    pub report: Option<PathBuf>,

#[arg(long, value_name = "PATH")]
    pub log: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct DumpUnpackedArgs {
    
    #[command(flatten)]
    pub launch: LaunchArgs,

#[arg(long, value_name = "PATH")]
    pub out: PathBuf,

#[arg(long, default_value = "TRAuth", value_name = "REGION")]
    pub region: String,

#[arg(long, default_value_t = 25, value_name = "MILLISECONDS")]
    pub snapshot_interval_ms: u64,

#[arg(long, default_value_t = 100, value_name = "MILLISECONDS")]
    pub quiescence_ms: u64,
}
