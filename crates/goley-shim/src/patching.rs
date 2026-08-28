

use std::{
    fs,
    path::{Path, PathBuf},
    ptr,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::{info, warn};
use windows::Win32::System::{
    Diagnostics::Debug::FlushInstructionCache,
    Memory::{PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect},
    Threading::GetCurrentProcess,
};

use crate::{config::ShimConfig, themida};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PatchManifest {
    
    pub schema_version: u32,
    
    #[serde(default)]
    pub patch: Vec<PatchRecord>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PatchRecord {
    
    pub rva: u32,
    
    pub original_bytes: String,
    
    pub patched_bytes: String,
    
    pub note: String,
    
    pub build_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatchApplyReport {
    
    pub build_sha256: String,
    
    pub applied: usize,
}

#[derive(Clone, Debug)]
struct DecodedPatch<'a> {
    record: &'a PatchRecord,
    original: Vec<u8>,
    patched: Vec<u8>,
}

pub fn load_manifest(path: &Path) -> Result<PatchManifest, PatchError> {
    let text = fs::read_to_string(path).map_err(|source| PatchError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let manifest: PatchManifest = toml::from_str(&text).map_err(PatchError::Toml)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn apply_configured(config: &ShimConfig) -> Result<PatchApplyReport, PatchError> {
    let executable = std::env::current_exe().map_err(PatchError::CurrentExecutable)?;
    let build_sha256 = sha256_file(&executable)?;
    let Some(path) = config.patches_path.as_deref() else {
        info!(
            event_type = "patch_summary",
            build_sha256,
            applied = 0,
            "no static patch manifest requested"
        );
        return Ok(PatchApplyReport {
            build_sha256,
            applied: 0,
        });
    };
    let manifest = load_manifest(path)?;
    let report = apply_manifest_for_build(&manifest, &build_sha256)?;
    info!(
        event_type = "patch_summary",
        build_sha256 = %report.build_sha256,
        applied = report.applied,
        manifest = %path.display(),
        "validated static patch set processed"
    );
    Ok(report)
}

pub fn apply_manifest_for_build(
    manifest: &PatchManifest,
    build_sha256: &str,
) -> Result<PatchApplyReport, PatchError> {
    validate_manifest(manifest)?;
    let normalized_build = normalize_sha256(build_sha256)?;
    let selected: Vec<_> = manifest
        .patch
        .iter()
        .filter(|record| record.build_sha256.eq_ignore_ascii_case(&normalized_build))
        .map(decode_record)
        .collect::<Result<_, _>>()?;

    if selected.is_empty() {
        if manifest.patch.is_empty() {
            return Ok(PatchApplyReport {
                build_sha256: normalized_build,
                applied: 0,
            });
        }
        return Err(PatchError::UnknownBuild(normalized_build));
    }

    let (base, image_size) = themida::current_image_layout()?;
    for patch in &selected {
        validate_mapped_original(base, image_size, patch)?;
    }
    for patch in &selected {
        write_patch(base, patch)?;
        info!(
            event_type = "static_patch",
            rva = patch.record.rva as u64,
            note = %patch.record.note,
            build_sha256 = %normalized_build,
            bytes = patch.patched.len(),
            "verified patch applied"
        );
    }
    Ok(PatchApplyReport {
        build_sha256: normalized_build,
        applied: selected.len(),
    })
}

fn validate_manifest(manifest: &PatchManifest) -> Result<(), PatchError> {
    if manifest.schema_version != 1 {
        return Err(PatchError::SchemaVersion(manifest.schema_version));
    }
    let mut ranges = Vec::with_capacity(manifest.patch.len());
    for record in &manifest.patch {
        let sha = normalize_sha256(&record.build_sha256)?;
        let decoded = decode_record(record)?;
        if decoded.original.is_empty() {
            return Err(PatchError::EmptyPatch { rva: record.rva });
        }
        if decoded.original.len() != decoded.patched.len() {
            return Err(PatchError::LengthMismatch {
                rva: record.rva,
                original: decoded.original.len(),
                patched: decoded.patched.len(),
            });
        }
        let end = (record.rva as usize)
            .checked_add(decoded.original.len())
            .ok_or(PatchError::AddressOverflow { rva: record.rva })?;
        ranges.push((sha, record.rva as usize, end));
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        let (left_sha, _left_start, left_end) = &pair[0];
        let (right_sha, right_start, _right_end) = &pair[1];
        if left_sha == right_sha && left_end > right_start {
            return Err(PatchError::Overlap {
                build_sha256: left_sha.clone(),
                rva: *right_start as u32,
            });
        }
    }
    Ok(())
}

fn decode_record(record: &PatchRecord) -> Result<DecodedPatch<'_>, PatchError> {
    Ok(DecodedPatch {
        record,
        original: decode_bytes(record.rva, "original_bytes", &record.original_bytes)?,
        patched: decode_bytes(record.rva, "patched_bytes", &record.patched_bytes)?,
    })
}

fn decode_bytes(rva: u32, field: &'static str, text: &str) -> Result<Vec<u8>, PatchError> {
    text.split_ascii_whitespace()
        .map(|byte| {
            if byte.len() != 2 {
                return Err(PatchError::InvalidBytes {
                    rva,
                    field,
                    value: text.to_owned(),
                });
            }
            u8::from_str_radix(byte, 16).map_err(|_| PatchError::InvalidBytes {
                rva,
                field,
                value: text.to_owned(),
            })
        })
        .collect()
}

fn normalize_sha256(value: &str) -> Result<String, PatchError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PatchError::InvalidSha256(value.to_owned()));
    }
    Ok(value.to_ascii_uppercase())
}

fn sha256_file(path: &Path) -> Result<String, PatchError> {
    let bytes = fs::read(path).map_err(|source| PatchError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(hex::encode_upper(Sha256::digest(bytes)))
}

fn validate_mapped_original(
    base: usize,
    image_size: u32,
    patch: &DecodedPatch<'_>,
) -> Result<(), PatchError> {
    let end = (patch.record.rva as usize)
        .checked_add(patch.original.len())
        .ok_or(PatchError::AddressOverflow {
            rva: patch.record.rva,
        })?;
    if end > image_size as usize {
        return Err(PatchError::OutsideImage {
            rva: patch.record.rva,
            length: patch.original.len(),
            image_size,
        });
    }
    let address = (base + patch.record.rva as usize) as *const u8;
    
    let actual = unsafe { std::slice::from_raw_parts(address, patch.original.len()) };
    if actual != patch.original {
        warn!(
            event_type = "patch_rejected",
            rva = patch.record.rva as u64,
            expected = %hex::encode_upper(&patch.original),
            actual = %hex::encode_upper(actual),
            "original bytes did not match"
        );
        return Err(PatchError::OriginalMismatch {
            rva: patch.record.rva,
            expected: hex::encode_upper(&patch.original),
            actual: hex::encode_upper(actual),
        });
    }
    Ok(())
}

fn write_patch(base: usize, patch: &DecodedPatch<'_>) -> Result<(), PatchError> {
    let address = (base + patch.record.rva as usize) as *mut u8;
    let mut old = PAGE_PROTECTION_FLAGS::default();

unsafe {
        VirtualProtect(
            address.cast(),
            patch.patched.len(),
            PAGE_EXECUTE_READWRITE,
            &mut old,
        )?;
        ptr::copy_nonoverlapping(patch.patched.as_ptr(), address, patch.patched.len());
        FlushInstructionCache(
            GetCurrentProcess(),
            Some(address.cast_const().cast()),
            patch.patched.len(),
        )?;
        let mut replaced = PAGE_PROTECTION_FLAGS::default();
        VirtualProtect(address.cast(), patch.patched.len(), old, &mut replaced)?;
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum PatchError {
    
    #[error("could not read {path}: {source}")]
    Read {
        
        path: PathBuf,
        
        source: std::io::Error,
    },
    
    #[error("could not identify current executable: {0}")]
    CurrentExecutable(std::io::Error),
    
    #[error("patch manifest TOML error: {0}")]
    Toml(toml::de::Error),
    
    #[error("unsupported patch schema version {0}")]
    SchemaVersion(u32),
    
    #[error("invalid SHA-256 value {0:?}")]
    InvalidSha256(String),
    
    #[error("invalid {field} at RVA 0x{rva:x}: {value:?}")]
    InvalidBytes {
        
        rva: u32,
        
        field: &'static str,
        
        value: String,
    },
    
    #[error("empty patch at RVA 0x{rva:x}")]
    EmptyPatch {
        
        rva: u32,
    },
    
    #[error("patch length mismatch at RVA 0x{rva:x}: original={original}, patched={patched}")]
    LengthMismatch {
        
        rva: u32,
        
        original: usize,
        
        patched: usize,
    },
    
    #[error("patch address overflow at RVA 0x{rva:x}")]
    AddressOverflow {
        
        rva: u32,
    },
    
    #[error("overlapping patch for build {build_sha256} at RVA 0x{rva:x}")]
    Overlap {
        
        build_sha256: String,
        
        rva: u32,
    },
    
    #[error("patch manifest has no records for build {0}")]
    UnknownBuild(String),
    
    #[error("patch at RVA 0x{rva:x} ({length} bytes) exceeds image size 0x{image_size:x}")]
    OutsideImage {
        
        rva: u32,
        
        length: usize,
        
        image_size: u32,
    },
    
    #[error("original bytes differ at RVA 0x{rva:x}: expected {expected}, actual {actual}")]
    OriginalMismatch {
        
        rva: u32,
        
        expected: String,
        
        actual: String,
    },
    
    #[error(transparent)]
    Themida(#[from] themida::ThemidaError),
    
    #[error("Windows patch operation failed: {0}")]
    Windows(#[from] windows::core::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_repository_manifest_is_valid() {
        let manifest = load_manifest(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/patches/patches.toml"
        )))
        .unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.patch.len(), 2);
        let patch = &manifest.patch[0];
        assert_eq!(patch.rva, 0x0093_74DB);
        assert_eq!(patch.original_bytes, "81 FE 55 07 00 00");
        assert_eq!(patch.patched_bytes, "81 FE 7C 01 00 00");
        assert_eq!(
            patch.build_sha256,
            "C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA"
        );
        let patch = &manifest.patch[1];
        assert_eq!(patch.rva, 0x0093_BB67);
        assert_eq!(patch.original_bytes, "E8 94 84 BA FF");
        assert_eq!(patch.patched_bytes, "B8 55 07 00 00");
        assert_eq!(
            patch.build_sha256,
            "C136E751905FBE60CF27A71D7D05FBDE7C0428484D577F3A0A56766C839EFCFA"
        );
    }

    #[test]
    fn rejects_different_patch_lengths() {
        let manifest = PatchManifest {
            schema_version: 1,
            patch: vec![PatchRecord {
                rva: 1,
                original_bytes: "90 90".to_owned(),
                patched_bytes: "90".to_owned(),
                note: "fixture".to_owned(),
                build_sha256: "00".repeat(32),
            }],
        };
        assert!(matches!(
            validate_manifest(&manifest),
            Err(PatchError::LengthMismatch { .. })
        ));
    }
}
