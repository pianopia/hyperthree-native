use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MAX_STORAGE_BYTES: usize = 5 * 1024 * 1024;

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

#[cfg(test)]
mod tests {
    use super::StorageStore;
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
}
