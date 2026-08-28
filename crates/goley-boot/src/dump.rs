

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};

use crate::sha256;

const DOS_MAGIC: &[u8; 2] = b"MZ";
const PE_MAGIC: &[u8; 4] = b"PE\0\0";
const PE32_MAGIC: u16 = 0x010b;
const MAX_SECTION_COUNT: usize = 96;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutableRange {
    pub(crate) rva: u32,
    pub(crate) length: u32,
    pub(crate) protection: u32,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CodeSnapshot {
    pub(crate) ranges: Vec<ExecutableRange>,
}

impl CodeSnapshot {
    pub(crate) fn ensure_usable(&self) -> Result<()> {
        ensure!(
            !self.ranges.is_empty(),
            "main image has no readable committed executable ranges"
        );
        ensure!(
            self.ranges.iter().any(|range| !range.bytes.is_empty()),
            "main image has no readable executable bytes"
        );
        Ok(())
    }

    pub(crate) fn sha256(&self) -> String {
        let capacity = self
            .ranges
            .iter()
            .map(|range| range.bytes.len().saturating_add(16))
            .sum();
        let mut canonical = Vec::with_capacity(capacity);
        for range in &self.ranges {
            canonical.extend_from_slice(&range.rva.to_le_bytes());
            canonical.extend_from_slice(&range.length.to_le_bytes());
            canonical.extend_from_slice(&range.protection.to_le_bytes());
            canonical.extend_from_slice(&(range.bytes.len() as u64).to_le_bytes());
            canonical.extend_from_slice(&range.bytes);
        }
        sha256::digest_hex(&canonical)
    }

    pub(crate) fn differing_ranges(&self, other: &Self) -> usize {
        let common = self.ranges.len().min(other.ranges.len());
        let changed = self.ranges[..common]
            .iter()
            .zip(&other.ranges[..common])
            .filter(|(left, right)| left != right)
            .count();
        changed + self.ranges.len().abs_diff(other.ranges.len())
    }
}

#[derive(Debug)]
pub(crate) struct MappedImage {
    pub(crate) base: usize,
    pub(crate) bytes: Vec<u8>,
    pub(crate) code: CodeSnapshot,
    pub(crate) memory_ranges: Vec<MemoryRange>,
    pub(crate) readable_bytes: usize,
    pub(crate) zero_filled_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MemoryRange {
    pub(crate) rva: u32,
    pub(crate) length: u32,
    pub(crate) committed: bool,
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pub(crate) executable: bool,
}

#[derive(Debug)]
pub(crate) struct StabilityTracker {
    baseline: CodeSnapshot,
    last: CodeSnapshot,
    transition_seen: bool,
    first_transition: Option<Duration>,
    last_change: Duration,
    change_samples: u64,
    maximum_changed_ranges: usize,
}

impl StabilityTracker {
    pub(crate) fn new(baseline: CodeSnapshot) -> Result<Self> {
        baseline.ensure_usable()?;
        Ok(Self {
            last: baseline.clone(),
            baseline,
            transition_seen: false,
            first_transition: None,
            last_change: Duration::ZERO,
            change_samples: 0,
            maximum_changed_ranges: 0,
        })
    }

pub(crate) fn observe(
        &mut self,
        sample: CodeSnapshot,
        elapsed: Duration,
        quiescence: Duration,
    ) -> bool {
        let changed_from_baseline = sample != self.baseline;
        if changed_from_baseline {
            self.transition_seen = true;
            self.first_transition.get_or_insert(elapsed);
            self.maximum_changed_ranges = self
                .maximum_changed_ranges
                .max(self.baseline.differing_ranges(&sample));
        }
        if sample != self.last {
            self.last = sample;
            self.last_change = elapsed;
            self.change_samples = self.change_samples.saturating_add(1);
        }

        self.transition_seen
            && self.last != self.baseline
            && elapsed.saturating_sub(self.last_change) >= quiescence
    }

    pub(crate) const fn transition_seen(&self) -> bool {
        self.transition_seen
    }

    pub(crate) const fn change_samples(&self) -> u64 {
        self.change_samples
    }

    pub(crate) const fn first_transition(&self) -> Option<Duration> {
        self.first_transition
    }

    pub(crate) const fn maximum_changed_ranges(&self) -> usize {
        self.maximum_changed_ranges
    }

    pub(crate) fn last(&self) -> &CodeSnapshot {
        &self.last
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RewriteFacts {
    pub(crate) section_count: usize,
    pub(crate) original_image_base: u32,
    pub(crate) captured_image_base: u32,
    pub(crate) synthesized_sections: bool,
}

#[derive(Debug)]
pub(crate) struct DumpWriteResult {
    pub(crate) path: PathBuf,
    pub(crate) sha256: String,
    pub(crate) size: usize,
    pub(crate) rewrite: RewriteFacts,
}

pub(crate) fn flatten_mapped_pe(
    image: &mut Vec<u8>,
    captured_base: usize,
    memory_ranges: &[MemoryRange],
) -> Result<RewriteFacts> {
    ensure!(image.len() >= 64, "mapped image has a truncated DOS header");
    ensure!(&image[..2] == DOS_MAGIC, "mapped image has no MZ signature");
    let nt_offset = read_u32(image, 0x3c)? as usize;
    ensure_range(image, nt_offset, 24, "PE header")?;
    ensure!(
        &image[nt_offset..nt_offset + 4] == PE_MAGIC,
        "mapped image has no PE signature"
    );

    let optional_size = usize::from(read_u16(image, nt_offset + 20)?);
    let optional = nt_offset + 24;
    ensure_range(image, optional, optional_size, "optional header")?;
    ensure!(
        read_u16(image, optional)? == PE32_MAGIC,
        "mapped image is not PE32"
    );
    ensure!(optional_size >= 96, "PE32 optional header is truncated");

    let declared_image_size = read_u32(image, optional + 56)? as usize;
    ensure!(
        declared_image_size != 0 && declared_image_size <= image.len(),
        "PE SizeOfImage 0x{declared_image_size:x} exceeds captured bytes 0x{:x}",
        image.len()
    );
    let file_alignment = read_u32(image, optional + 36)?;
    ensure!(
        file_alignment.is_power_of_two() && (0x200..=0x1_0000).contains(&file_alignment),
        "invalid PE FileAlignment 0x{file_alignment:x}"
    );
    let section_alignment = read_u32(image, optional + 32)?;
    ensure!(
        section_alignment.is_power_of_two()
            && (file_alignment..=0x10_0000).contains(&section_alignment),
        "invalid PE SectionAlignment 0x{section_alignment:x}"
    );
    let size_of_headers = read_u32(image, optional + 60)? as usize;
    ensure!(
        size_of_headers <= declared_image_size,
        "PE SizeOfHeaders 0x{size_of_headers:x} exceeds SizeOfImage"
    );
    let captured_image_base = u32::try_from(captured_base)
        .context("captured x86 image base does not fit in a PE32 ImageBase")?;
    let original_image_base = read_u32(image, optional + 28)?;
    write_u32(image, optional + 28, captured_image_base)?;

let directory_count = read_u32(image, optional + 92)? as usize;
    if directory_count > 4 && optional_size >= 96 + 5 * 8 {
        write_u32(image, optional + 96 + 4 * 8, 0)?;
        write_u32(image, optional + 96 + 4 * 8 + 4, 0)?;
    }
    
    write_u32(image, optional + 64, 0)?;

    let sections = optional
        .checked_add(optional_size)
        .context("section-table offset overflow")?;
    ensure!(
        sections < size_of_headers,
        "PE headers leave no room for an analysis section table"
    );
    let first_section_rva = align_up(size_of_headers, section_alignment as usize)?;
    let analysis_ranges = analysis_ranges(memory_ranges, first_section_rva, declared_image_size)?;
    let section_count = analysis_ranges.len();
    let table_capacity = (size_of_headers - sections) / 40;
    ensure!(
        (1..=MAX_SECTION_COUNT.min(table_capacity)).contains(&section_count),
        "measured memory map needs {section_count} analysis sections, but PE headers hold {table_capacity}"
    );
    image[sections..size_of_headers].fill(0);
    write_u16(image, nt_offset + 6, u16::try_from(section_count)?)?;

    let mut required_file_size = declared_image_size;
    let mut size_of_code = 0_usize;
    let mut size_of_initialized_data = 0_usize;
    let mut base_of_code = None;
    let mut base_of_data = None;
    for (index, range) in analysis_ranges.iter().enumerate() {
        let header = sections + index * 40;
        let name = format!(".m{index:04X}");
        image[header..header + name.len()].copy_from_slice(name.as_bytes());
        write_u32(image, header + 8, range.length)?;
        write_u32(image, header + 12, range.rva)?;
        let raw_size = align_up(range.length as usize, file_alignment as usize)?;
        let raw_end = (range.rva as usize)
            .checked_add(raw_size)
            .context("flat section extent overflow")?;
        required_file_size = required_file_size.max(raw_end);
        write_u32(image, header + 16, u32::try_from(raw_size)?)?;
        write_u32(image, header + 20, range.rva)?;
        let mut characteristics = if range.executable {
            0x0000_0020_u32 
        } else {
            0x0000_0040_u32 
        };
        if range.readable {
            characteristics |= 0x4000_0000; 
        }
        if range.writable {
            characteristics |= 0x8000_0000; 
        }
        if range.executable {
            characteristics |= 0x2000_0000; 
            size_of_code = size_of_code
                .checked_add(raw_size)
                .context("SizeOfCode overflow")?;
            base_of_code.get_or_insert(range.rva);
        } else {
            size_of_initialized_data = size_of_initialized_data
                .checked_add(raw_size)
                .context("SizeOfInitializedData overflow")?;
            base_of_data.get_or_insert(range.rva);
        }
        write_u32(image, header + 36, characteristics)?;
    }
    write_u32(image, optional + 4, u32::try_from(size_of_code)?)?;
    write_u32(
        image,
        optional + 8,
        u32::try_from(size_of_initialized_data)?,
    )?;
    write_u32(image, optional + 12, 0)?;
    write_u32(image, optional + 20, base_of_code.unwrap_or(0))?;
    write_u32(image, optional + 24, base_of_data.unwrap_or(0))?;
    image.resize(required_file_size, 0);

    Ok(RewriteFacts {
        section_count,
        original_image_base,
        captured_image_base,
        synthesized_sections: true,
    })
}

fn analysis_ranges(
    memory_ranges: &[MemoryRange],
    first_rva: usize,
    image_size: usize,
) -> Result<Vec<MemoryRange>> {
    let mut output: Vec<MemoryRange> = Vec::new();
    for measured in memory_ranges.iter().filter(|range| range.committed) {
        let measured_start = measured.rva as usize;
        let measured_end = measured_start
            .checked_add(measured.length as usize)
            .context("measured memory-range extent overflow")?
            .min(image_size);
        let start = measured_start.max(first_rva);
        if start >= measured_end {
            continue;
        }
        let clipped = MemoryRange {
            rva: u32::try_from(start)?,
            length: u32::try_from(measured_end - start)?,
            committed: true,
            readable: measured.readable,
            writable: measured.writable,
            executable: measured.executable,
        };
        if let Some(previous) = output.last_mut() {
            let previous_end = previous.rva as usize + previous.length as usize;
            if previous_end == start
                && previous.readable == clipped.readable
                && previous.writable == clipped.writable
                && previous.executable == clipped.executable
            {
                previous.length = previous
                    .length
                    .checked_add(clipped.length)
                    .context("coalesced memory-range length overflow")?;
                continue;
            }
        }
        output.push(clipped);
    }
    Ok(output)
}

pub(crate) fn write_dump(destination: &Path, mut mapped: MappedImage) -> Result<DumpWriteResult> {
    ensure!(
        mapped
            .readable_bytes
            .saturating_add(mapped.zero_filled_bytes)
            == mapped.bytes.len(),
        "mapped-image coverage counters do not cover SizeOfImage"
    );
    mapped.code.ensure_usable()?;
    let destination = validated_destination(destination)?;
    let rewrite = flatten_mapped_pe(&mut mapped.bytes, mapped.base, &mapped.memory_ranges)?;
    let sha256 = sha256::digest_hex(&mapped.bytes);
    let size = mapped.bytes.len();

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let file_name = destination
        .file_name()
        .context("dump destination has no file name")?
        .to_string_lossy();
    let temporary = destination.with_file_name(format!(
        ".{file_name}.partial-{}-{unique:x}",
        std::process::id()
    ));
    let write_result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("failed to create temporary dump {}", temporary.display()))?;
        file.write_all(&mapped.bytes)
            .with_context(|| format!("failed to write temporary dump {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to flush temporary dump {}", temporary.display()))?;
        drop(file);
        fs::rename(&temporary, &destination).with_context(|| {
            format!(
                "failed to atomically publish dump {}",
                destination.display()
            )
        })?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;

    Ok(DumpWriteResult {
        path: destination,
        sha256,
        size,
        rewrite,
    })
}

pub(crate) fn preflight_destination(destination: &Path) -> Result<PathBuf> {
    validated_destination(destination)
}

fn validated_destination(destination: &Path) -> Result<PathBuf> {
    ensure!(
        destination.file_name().is_some(),
        "dump destination must name a file"
    );
    ensure!(
        !destination.exists(),
        "dump destination already exists; refusing to overwrite {}",
        destination.display()
    );
    let absolute = if destination.is_absolute() {
        lexical_normalize(destination)
    } else {
        lexical_normalize(&std::env::current_dir()?.join(destination))
    };
    let parent = absolute
        .parent()
        .context("dump destination has no parent directory")?;
    ensure!(
        parent.is_dir(),
        "dump parent directory does not exist: {}",
        parent.display()
    );
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("failed to canonicalize dump parent {}", parent.display()))?;
    let file_name = absolute
        .file_name()
        .context("dump destination has no file name")?;
    let canonical_destination = canonical_parent.join(file_name);

    let workspace_path = workspace_root()?;
    let workspace = workspace_path.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize workspace root {}",
            workspace_path.display()
        )
    })?;
    ensure!(
        !canonical_destination.starts_with(&workspace),
        "dump output must stay outside the repository: {}",
        workspace.display()
    );
    Ok(canonical_destination)
}

fn workspace_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        let cargo = ancestor.join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&cargo)
            && text.contains("[workspace]")
        {
            return Ok(ancestor.to_path_buf());
        }
    }
    bail!(
        "could not locate workspace root above {}",
        manifest.display()
    )
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn align_up(value: usize, alignment: usize) -> Result<usize> {
    let mask = alignment - 1;
    value
        .checked_add(mask)
        .map(|sum| sum & !mask)
        .context("aligned PE extent overflow")
}

fn ensure_range(bytes: &[u8], offset: usize, length: usize, label: &str) -> Result<()> {
    let end = offset
        .checked_add(length)
        .with_context(|| format!("{label} range overflow"))?;
    ensure!(end <= bytes.len(), "{label} exceeds mapped image");
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    ensure_range(bytes, offset, 2, "u16 field")?;
    Ok(u16::from_le_bytes(
        bytes[offset..offset + 2].try_into().expect("fixed slice"),
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    ensure_range(bytes, offset, 4, "u32 field")?;
    Ok(u32::from_le_bytes(
        bytes[offset..offset + 4].try_into().expect("fixed slice"),
    ))
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<()> {
    ensure_range(bytes, offset, 4, "u32 field")?;
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<()> {
    ensure_range(bytes, offset, 2, "u16 field")?;
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(byte: u8) -> CodeSnapshot {
        CodeSnapshot {
            ranges: vec![ExecutableRange {
                rva: 0x1000,
                length: 16,
                protection: 0x20,
                bytes: vec![byte; 16],
            }],
        }
    }

    #[test]
    fn stability_requires_transition_and_full_quiet_period() {
        let mut tracker = StabilityTracker::new(code(1)).unwrap();
        assert!(!tracker.observe(
            code(1),
            Duration::from_millis(2_000),
            Duration::from_millis(500)
        ));
        assert!(!tracker.observe(
            code(2),
            Duration::from_millis(2_100),
            Duration::from_millis(500)
        ));
        assert!(!tracker.observe(
            code(2),
            Duration::from_millis(2_599),
            Duration::from_millis(500)
        ));
        assert!(tracker.observe(
            code(2),
            Duration::from_millis(2_600),
            Duration::from_millis(500)
        ));
        assert!(tracker.transition_seen());
        assert_eq!(tracker.change_samples(), 1);
        assert_eq!(tracker.maximum_changed_ranges(), 1);
    }

    #[test]
    fn transition_that_returns_to_baseline_is_not_ready() {
        let mut tracker = StabilityTracker::new(code(1)).unwrap();
        assert!(!tracker.observe(
            code(2),
            Duration::from_millis(10),
            Duration::from_millis(10)
        ));
        assert!(!tracker.observe(
            code(1),
            Duration::from_millis(100),
            Duration::from_millis(10)
        ));
    }

    #[test]
    fn repository_destination_is_rejected_before_capture() {
        let destination = workspace_root()
            .unwrap()
            .join("goley-dump-must-stay-local.test-dump");
        let error = preflight_destination(&destination).unwrap_err();
        assert!(error.to_string().contains("outside the repository"));
    }

    #[test]
    fn flat_pe_replaces_destroyed_section_table_without_guessing_oep() {
        let mut image = vec![0_u8; 0x3000];
        image[..2].copy_from_slice(DOS_MAGIC);
        image[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
        image[0x80..0x84].copy_from_slice(PE_MAGIC);
        image[0x84..0x86].copy_from_slice(&0x014c_u16.to_le_bytes());
        image[0x86..0x88].copy_from_slice(&1_u16.to_le_bytes());
        image[0x94..0x96].copy_from_slice(&0xe0_u16.to_le_bytes());
        let optional = 0x98;
        image[optional..optional + 2].copy_from_slice(&PE32_MAGIC.to_le_bytes());
        image[optional + 16..optional + 20].copy_from_slice(&0x1234_u32.to_le_bytes());
        image[optional + 28..optional + 32].copy_from_slice(&0x400000_u32.to_le_bytes());
        image[optional + 32..optional + 36].copy_from_slice(&0x1000_u32.to_le_bytes());
        image[optional + 36..optional + 40].copy_from_slice(&0x200_u32.to_le_bytes());
        image[optional + 56..optional + 60].copy_from_slice(&0x3000_u32.to_le_bytes());
        image[optional + 60..optional + 64].copy_from_slice(&0x400_u32.to_le_bytes());
        image[optional + 92..optional + 96].copy_from_slice(&16_u32.to_le_bytes());
        let section = optional + 0xe0;
        image[section..section + 5].copy_from_slice(b".text");
        image[section + 8..section + 12].copy_from_slice(&0x600_u32.to_le_bytes());

image[section + 12..section + 16].copy_from_slice(&0xad08_2188_u32.to_le_bytes());
        image[section + 16..section + 20].copy_from_slice(&0x200_u32.to_le_bytes());
        image[section + 20..section + 24].copy_from_slice(&0x400_u32.to_le_bytes());

        let memory_ranges = [MemoryRange {
            rva: 0x1000,
            length: 0x600,
            committed: true,
            readable: true,
            writable: false,
            executable: true,
        }];
        let facts = flatten_mapped_pe(&mut image, 0x500000, &memory_ranges).unwrap();

        assert_eq!(facts.section_count, 1);
        assert_eq!(facts.original_image_base, 0x400000);
        assert_eq!(facts.captured_image_base, 0x500000);
        assert!(facts.synthesized_sections);
        assert_eq!(read_u32(&image, optional + 16).unwrap(), 0x1234);
        assert_eq!(read_u32(&image, section + 16).unwrap(), 0x600);
        assert_eq!(read_u32(&image, section + 20).unwrap(), 0x1000);
    }
}
