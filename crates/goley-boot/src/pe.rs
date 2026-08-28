

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

const DOS_HEADER_SIZE: usize = 64;
const IMAGE_FILE_MACHINE_I386: u16 = 0x014c;
const PE32_MAGIC: u16 = 0x010b;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PeInfo {
    
    pub machine: u16,
    
    pub optional_header_magic: u16,
}

impl PeInfo {
    
    #[must_use]
    pub const fn is_x86_pe32(self) -> bool {
        self.machine == IMAGE_FILE_MACHINE_I386 && self.optional_header_magic == PE32_MAGIC
    }
}

pub fn inspect(path: &Path) -> Result<PeInfo> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open client image {}", path.display()))?;
    let mut dos = [0_u8; DOS_HEADER_SIZE];
    file.read_exact(&mut dos)
        .with_context(|| format!("{} has a truncated DOS header", path.display()))?;
    ensure!(&dos[..2] == b"MZ", "{} is not an MZ image", path.display());

    let pe_offset = u32::from_le_bytes(dos[0x3c..0x40].try_into().expect("fixed slice"));
    ensure!(
        pe_offset >= DOS_HEADER_SIZE as u32,
        "{} has an invalid PE offset",
        path.display()
    );
    file.seek(SeekFrom::Start(u64::from(pe_offset)))?;

    let mut header = [0_u8; 26];
    file.read_exact(&mut header)
        .with_context(|| format!("{} has a truncated PE header", path.display()))?;
    ensure!(
        &header[..4] == b"PE\0\0",
        "{} has no PE signature",
        path.display()
    );

    let machine = u16::from_le_bytes([header[4], header[5]]);
    let optional_header_size = u16::from_le_bytes([header[20], header[21]]);
    if optional_header_size < 2 {
        bail!("{} has no usable optional header", path.display());
    }
    let optional_header_magic = u16::from_le_bytes([header[24], header[25]]);
    Ok(PeInfo {
        machine,
        optional_header_magic,
    })
}

pub fn require_x86_client(path: &Path) -> Result<PeInfo> {
    let info = inspect(path)?;
    ensure!(
        info.is_x86_pe32(),
        "client must be x86 PE32 (machine=0x014c, magic=0x010b); found machine=0x{:04x}, magic=0x{:04x}",
        info.machine,
        info.optional_header_magic
    );
    Ok(info)
}
