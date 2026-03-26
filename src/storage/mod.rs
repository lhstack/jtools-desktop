use crate::search::SearchItem;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root_dir: PathBuf,
    pub data_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub settings_file: PathBuf,
    pub recent_file: PathBuf,
    pub plugin_registry_file: PathBuf,
    pub removed_builtin_file: PathBuf,
    pub search_timeout_ms: u64,
}

impl AppPaths {
    pub fn new(root_dir: impl AsRef<Path>) -> Self {
        let root_dir = root_dir.as_ref().to_path_buf();
        let data_dir = root_dir.join("data");
        let plugins_dir = root_dir.join("plugins");
        Self {
            root_dir,
            settings_file: data_dir.join("settings.json"),
            recent_file: data_dir.join("recent.json"),
            plugin_registry_file: data_dir.join("plugin-registry.json"),
            removed_builtin_file: data_dir.join("removed-builtin.json"),
            data_dir,
            plugins_dir,
            search_timeout_ms: 500,
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsonStore<T> {
    path: PathBuf,
    marker: std::marker::PhantomData<T>,
}

impl<T> JsonStore<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            marker: std::marker::PhantomData,
        }
    }

    pub async fn load_or_default(&self) -> Result<T> {
        if !self.path.exists() {
            return Ok(T::default());
        }
        let content = tokio::fs::read_to_string(&self.path)
            .await
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let value = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;
        Ok(value)
    }

    pub async fn save(&self, value: &T) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            ensure_dir(parent).await?;
        }
        let content = serde_json::to_string_pretty(value)?;
        tokio::fs::write(&self.path, content)
            .await
            .with_context(|| format!("failed to write {}", self.path.display()))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecentEntry {
    pub key: String,
    pub item: SearchItem,
    pub used_at: DateTime<Utc>,
}

impl RecentEntry {
    pub fn from_search_item(item: &SearchItem) -> Self {
        Self {
            key: item.key(),
            item: item.clone(),
            used_at: Utc::now(),
        }
    }
}

pub async fn ensure_dir(path: impl AsRef<Path>) -> Result<()> {
    tokio::fs::create_dir_all(path.as_ref()).await?;
    Ok(())
}

pub async fn copy_dir_recursive(source: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<()> {
    ensure_dir(&target).await?;
    let mut entries = tokio::fs::read_dir(source.as_ref()).await?;
    while let Some(entry) = entries.next_entry().await? {
        let entry_path = entry.path();
        let target_path = target.as_ref().join(entry.file_name());
        if entry.file_type().await?.is_dir() {
            Box::pin(copy_dir_recursive(&entry_path, &target_path)).await?;
        } else {
            tokio::fs::copy(&entry_path, &target_path).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::JsonStore;
    use anyhow::Result;
    use tempfile::TempDir;

    #[tokio::test]
    async fn json_store_round_trip() -> Result<()> {
        let temp = TempDir::new()?;
        let store = JsonStore::<Vec<String>>::new(temp.path().join("items.json"));
        store.save(&vec!["a".into(), "b".into()]).await?;
        let items = store.load_or_default().await?;
        assert_eq!(items, vec!["a", "b"]);
        Ok(())
    }
}
