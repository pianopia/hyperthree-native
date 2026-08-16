use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_STORAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_SANDBOX_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Project-scoped key/value storage used by the browser-compatible
/// `localStorage` surface. The file never escapes the project root, which
/// keeps game persistence separate from arbitrary host filesystem access.
pub struct StorageStore {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl StorageStore {
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self> {
        let path = project_root
            .as_ref()
            .join(".hyperthree")
            .join("storage")
            .join("local-storage.json");
        let values = if path.exists() {
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read storage file {}", path.display()))?;
            if bytes.len() > MAX_STORAGE_BYTES {
                anyhow::bail!("local storage exceeds {MAX_STORAGE_BYTES} bytes");
            }
            parse_values(&bytes)
                .with_context(|| format!("invalid storage file {}", path.display()))?
        } else {
            BTreeMap::new()
        };
        Ok(Self { path, values })
    }

    pub fn snapshot_json(&self) -> Result<String> {
        serde_json::to_string(&self.values).context("failed to serialize local storage")
    }

    pub fn replace_json(&mut self, payload: &str) -> Result<()> {
        if payload.len() > MAX_STORAGE_BYTES {
            anyhow::bail!("local storage exceeds {MAX_STORAGE_BYTES} bytes");
        }
        let values = parse_values(payload.as_bytes()).context("invalid local storage payload")?;
        self.persist(&values)?;
        self.values = values;
        Ok(())
    }

    fn persist(&self, values: &BTreeMap<String, String>) -> Result<()> {
        let payload = serde_json::to_vec(values).context("failed to serialize local storage")?;
        if payload.len() > MAX_STORAGE_BYTES {
            anyhow::bail!("local storage exceeds {MAX_STORAGE_BYTES} bytes");
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create storage directory {}", parent.display())
            })?;
        }
        let temporary_path = self.path.with_extension("json.tmp");
        fs::write(&temporary_path, payload).with_context(|| {
            format!("failed to write storage file {}", temporary_path.display())
        })?;
        if cfg!(windows) && self.path.exists() {
            fs::remove_file(&self.path).with_context(|| {
                format!("failed to replace storage file {}", self.path.display())
            })?;
        }
        fs::rename(&temporary_path, &self.path)
            .with_context(|| format!("failed to commit storage file {}", self.path.display()))?;
        Ok(())
    }
}

fn parse_values(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let value: Value = serde_json::from_slice(bytes).context("storage must be valid JSON")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("storage root must be a JSON object"))?;
    let mut values = BTreeMap::new();
    for (key, value) in object {
        let value = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("storage value for {key:?} must be a string"))?;
        values.insert(key.clone(), value.to_string());
    }
    Ok(values)
}

/// Origin-private file storage for the File System Access API compatibility
/// surface. Every path is relative to `.hyperthree/files` and is validated
/// before any host filesystem operation.
pub struct SandboxFileStore {
    root: PathBuf,
}

impl SandboxFileStore {
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self> {
        let root = project_root.as_ref().join(".hyperthree").join("files");
        fs::create_dir_all(&root)
            .with_context(|| format!("failed to create sandbox directory {}", root.display()))?;
        Ok(Self {
            root: root
                .canonicalize()
                .with_context(|| format!("failed to canonicalize sandbox {}", root.display()))?,
        })
    }

    pub fn read(&self, relative_path: &str) -> Result<Vec<u8>> {
        anyhow::ensure!(!relative_path.is_empty(), "sandbox path must not be empty");
        let path = self.existing_path(relative_path)?;
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read sandbox file {relative_path}"))?;
        if bytes.len() > MAX_SANDBOX_FILE_BYTES {
            anyhow::bail!("sandbox file exceeds {MAX_SANDBOX_FILE_BYTES} bytes");
        }
        Ok(bytes)
    }

    pub fn write(&self, relative_path: &str, bytes: &[u8]) -> Result<()> {
        anyhow::ensure!(!relative_path.is_empty(), "sandbox path must not be empty");
        if bytes.len() > MAX_SANDBOX_FILE_BYTES {
            anyhow::bail!("sandbox file exceeds {MAX_SANDBOX_FILE_BYTES} bytes");
        }
        let path = self.new_path(relative_path)?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("sandbox file has no parent directory"))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create sandbox parent {}", parent.display()))?;
        self.ensure_inside(parent)?;
        fs::write(&path, bytes)
            .with_context(|| format!("failed to write sandbox file {relative_path}"))?;
        Ok(())
    }

    pub fn remove(&self, relative_path: &str, recursive: bool) -> Result<()> {
        anyhow::ensure!(!relative_path.is_empty(), "cannot remove the sandbox root");
        let path = self.existing_path(relative_path)?;
        if path.is_dir() {
            anyhow::ensure!(
                recursive,
                "cannot remove a non-empty directory without recursive=true"
            );
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to remove sandbox directory {relative_path}"))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove sandbox file {relative_path}"))?;
        }
        Ok(())
    }

    pub fn list(&self, relative_path: &str) -> Result<Vec<(String, bool)>> {
        let path = self.existing_path(relative_path)?;
        anyhow::ensure!(
            path.is_dir(),
            "sandbox path is not a directory: {relative_path}"
        );
        let mut entries = Vec::new();
        for entry in fs::read_dir(&path)
            .with_context(|| format!("failed to list sandbox directory {relative_path}"))?
        {
            let entry = entry.context("failed to read sandbox directory entry")?;
            let name = entry.file_name().to_string_lossy().into_owned();
            entries.push((name, entry.file_type()?.is_dir()));
        }
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(entries)
    }

    fn existing_path(&self, relative_path: &str) -> Result<PathBuf> {
        let path = self.new_path(relative_path)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("sandbox path does not exist: {relative_path}"))?;
        self.ensure_inside(&canonical)?;
        Ok(canonical)
    }

    fn new_path(&self, relative_path: &str) -> Result<PathBuf> {
        if relative_path.is_empty() {
            return Ok(self.root.clone());
        }
        let relative = validate_sandbox_path(relative_path)?;
        Ok(self.root.join(relative))
    }

    fn ensure_inside(&self, path: &Path) -> Result<()> {
        anyhow::ensure!(
            path.starts_with(&self.root),
            "sandbox path escapes the project-private directory"
        );
        Ok(())
    }
}

fn validate_sandbox_path(path: &str) -> Result<PathBuf> {
    let relative = PathBuf::from(path);
    anyhow::ensure!(!path.is_empty(), "sandbox path must not be empty");
    anyhow::ensure!(!relative.is_absolute(), "sandbox path must be relative");
    anyhow::ensure!(
        relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_))),
        "sandbox path must not contain traversal or parent components"
    );
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::{SandboxFileStore, StorageStore};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn persists_string_values_inside_project_sandbox() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-storage-test-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        let mut store = StorageStore::new(&root).unwrap();
        store
            .replace_json(r#"{"score":"42","name":"pilot"}"#)
            .unwrap();
        assert_eq!(
            store.snapshot_json().unwrap(),
            r#"{"name":"pilot","score":"42"}"#
        );
        let restored = StorageStore::new(&root).unwrap();
        assert_eq!(
            restored.snapshot_json().unwrap(),
            r#"{"name":"pilot","score":"42"}"#
        );
        assert!(root
            .join(".hyperthree/storage/local-storage.json")
            .starts_with(&root));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_writes_lists_and_removes_origin_private_files() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hyperthree-files-test-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        let store = SandboxFileStore::new(&root).unwrap();
        store.write("saves/slot.bin", &[1, 2, 3]).unwrap();
        assert_eq!(store.read("saves/slot.bin").unwrap(), vec![1, 2, 3]);
        assert_eq!(
            store.list("saves").unwrap(),
            vec![("slot.bin".to_string(), false)]
        );
        assert!(store.read("../outside").is_err());
        store.remove("saves/slot.bin", false).unwrap();
        assert!(store.read("saves/slot.bin").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
