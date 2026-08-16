use anyhow::{Context, Result};
use memmap2::{Mmap, MmapOptions};
use std::{fs::File, path::Path};

/// Memory-mapped asset storage. The mapped bytes can be handed to a native
/// decoder without first copying them through a JavaScript ArrayBuffer.
pub struct MappedAsset {
    map: Mmap,
}

impl MappedAsset {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file =
            File::open(path).with_context(|| format!("failed to open asset {}", path.display()))?;
        // The file descriptor remains owned by the OS after mapping. Keeping
        // the mapping read-only prevents accidental mutation of source data.
        let map = unsafe { MmapOptions::new().map(&file) }
            .with_context(|| format!("failed to mmap asset {}", path.display()))?;
        Ok(Self { map })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.map
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
}
