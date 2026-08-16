use anyhow::{Context, Result};
use memmap2::{Mmap, MmapOptions};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Memory-mapped asset storage. The mapped bytes can be handed to a native
/// decoder without first copying them through a JavaScript ArrayBuffer.
#[derive(Debug)]
pub struct MappedAsset {
    map: Mmap,
}

#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub relative_path: String,
    pub byte_length: usize,
    pub format: String,
    pub mesh_count: usize,
    pub primitive_count: usize,
    pub animation_count: usize,
}

#[derive(Debug)]
pub struct AssetStore {
    root: PathBuf,
    mapped: HashMap<PathBuf, Arc<MappedAsset>>,
}

impl AssetStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("asset root does not exist: {}", root.as_ref().display()))?;
        Ok(Self {
            root,
            mapped: HashMap::new(),
        })
    }

    pub fn load(&mut self, relative_path: &str) -> Result<AssetMetadata> {
        let relative = validate_relative_path(relative_path)?;
        let path = self.root.join(&relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("asset does not exist: {relative_path}"))?;
        anyhow::ensure!(
            canonical.starts_with(&self.root),
            "asset path escapes project root: {relative_path}"
        );
        if !self.mapped.contains_key(&canonical) {
            self.mapped
                .insert(canonical.clone(), Arc::new(MappedAsset::open(&canonical)?));
        }
        let mapped = self
            .mapped
            .get(&canonical)
            .expect("asset mapping was inserted")
            .clone();
        let inspected = inspect_asset(mapped.bytes(), &canonical)?;
        Ok(AssetMetadata {
            relative_path: relative.to_string_lossy().replace('\\', "/"),
            byte_length: mapped.len(),
            format: inspected.format,
            mesh_count: inspected.mesh_count,
            primitive_count: inspected.primitive_count,
            animation_count: inspected.animation_count,
        })
    }
}

#[derive(Debug, Default)]
struct InspectedAsset {
    format: String,
    mesh_count: usize,
    primitive_count: usize,
    animation_count: usize,
}

fn validate_relative_path(path: &str) -> Result<PathBuf> {
    anyhow::ensure!(!path.is_empty(), "asset path must not be empty");
    let relative = PathBuf::from(path);
    anyhow::ensure!(!relative.is_absolute(), "asset path must be relative");
    anyhow::ensure!(
        relative
            .components()
            .all(|component| { !matches!(component, std::path::Component::ParentDir) }),
        "asset path must not contain parent traversal"
    );
    Ok(relative)
}

fn inspect_asset(bytes: &[u8], path: &Path) -> Result<InspectedAsset> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let is_glb = bytes.get(0..4) == Some(b"glTF");
    if extension == "gltf" || extension == "glb" || is_glb {
        let document = gltf::Gltf::from_slice(bytes)
            .with_context(|| format!("failed to parse glTF asset {}", path.display()))?;
        return Ok(InspectedAsset {
            format: if is_glb || extension == "glb" {
                "glb".to_string()
            } else {
                "gltf".to_string()
            },
            mesh_count: document.meshes().count(),
            primitive_count: document
                .meshes()
                .map(|mesh| mesh.primitives().count())
                .sum(),
            animation_count: document.animations().count(),
        });
    }
    Ok(InspectedAsset {
        format: if extension.is_empty() {
            "binary".to_string()
        } else {
            extension
        },
        ..Default::default()
    })
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

#[cfg(test)]
mod tests {
    use super::AssetStore;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn loads_project_relative_binary_and_reports_metadata() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-asset-test-{suffix}"));
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(root.join("public/test.bin"), [1_u8, 2, 3, 4]).unwrap();
        fs::write(
            root.join("public/scene.gltf"),
            br#"{
              "asset": {"version": "2.0"},
              "buffers": [{"byteLength": 36, "uri": "data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}],
              "bufferViews": [{"buffer": 0, "byteLength": 36}],
              "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 1]}],
              "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
              "animations": [{"channels": [], "samplers": []}]
            }"#,
        )
        .unwrap();
        let mut store = AssetStore::new(&root).unwrap();
        let metadata = store.load("public/test.bin").unwrap();
        assert_eq!(metadata.byte_length, 4);
        assert_eq!(metadata.format, "bin");
        let gltf = store.load("public/scene.gltf").unwrap();
        assert_eq!(gltf.format, "gltf");
        assert_eq!(gltf.mesh_count, 1);
        assert_eq!(gltf.primitive_count, 1);
        assert_eq!(gltf.animation_count, 1);
        assert!(store.load("../outside.bin").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
