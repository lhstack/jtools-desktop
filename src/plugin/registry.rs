use crate::plugin::manifest::PluginManifest;
use crate::storage::{JsonStore, copy_dir_recursive};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Enabled,
    Disabled,
    Faulted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub plugin_id: String,
    pub version: String,
    pub install_path: PathBuf,
    pub status: PluginStatus,
    pub permissions: Vec<String>,
    pub checksum: Option<String>,
    pub installed_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct RegisteredPlugin {
    pub manifest: PluginManifest,
    pub metadata: PluginMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginRegistrySummary {
    pub total: usize,
    pub enabled: usize,
    pub disabled: usize,
    pub faulted: usize,
}

pub struct PluginRegistry {
    items: HashMap<String, RegisteredPlugin>,
    metadata_store: JsonStore<Vec<PluginMetadata>>,
}

impl PluginRegistry {
    pub async fn scan(
        plugins_dir: impl AsRef<Path>,
        metadata_store: JsonStore<Vec<PluginMetadata>>,
    ) -> Result<Self> {
        let plugins_dir = plugins_dir.as_ref();
        let stored = metadata_store.load_or_default().await?;
        let mut metadata_by_id: HashMap<String, PluginMetadata> = stored
            .into_iter()
            .map(|item| (item.plugin_id.clone(), item))
            .collect();
        let mut items = HashMap::new();

        let mut read_dir = tokio::fs::read_dir(plugins_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }

            let plugin_dir = entry.path();
            let manifest_path = plugin_dir.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }

            let content = tokio::fs::read_to_string(&manifest_path)
                .await
                .with_context(|| format!("failed to read {}", manifest_path.display()))?;
            let manifest: PluginManifest = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
            manifest.validate()?;

            let now = Utc::now();
            let metadata = metadata_by_id
                .remove(&manifest.id)
                .unwrap_or_else(|| PluginMetadata {
                    plugin_id: manifest.id.clone(),
                    version: manifest.version.clone(),
                    install_path: plugin_dir.clone(),
                    status: PluginStatus::Enabled,
                    permissions: manifest.permissions.clone(),
                    checksum: None,
                    installed_at: now,
                    updated_at: now,
                });

            items.insert(
                manifest.id.clone(),
                RegisteredPlugin {
                    manifest,
                    metadata: PluginMetadata {
                        version: metadata.version,
                        install_path: plugin_dir,
                        updated_at: now,
                        permissions: metadata.permissions,
                        plugin_id: metadata.plugin_id,
                        status: metadata.status,
                        checksum: metadata.checksum,
                        installed_at: metadata.installed_at,
                    },
                },
            );
        }

        let registry = Self {
            items,
            metadata_store,
        };
        registry.persist_metadata().await?;
        Ok(registry)
    }

    pub fn enabled_plugins(&self) -> Vec<RegisteredPlugin> {
        self.items
            .values()
            .filter(|plugin| plugin.metadata.status == PluginStatus::Enabled)
            .cloned()
            .collect()
    }

    pub fn all_plugins(&self) -> Vec<RegisteredPlugin> {
        self.items.values().cloned().collect()
    }

    pub fn summary(&self) -> PluginRegistrySummary {
        let total = self.items.len();
        let enabled = self
            .items
            .values()
            .filter(|plugin| plugin.metadata.status == PluginStatus::Enabled)
            .count();
        let disabled = self
            .items
            .values()
            .filter(|plugin| plugin.metadata.status == PluginStatus::Disabled)
            .count();
        let faulted = self
            .items
            .values()
            .filter(|plugin| plugin.metadata.status == PluginStatus::Faulted)
            .count();

        PluginRegistrySummary {
            total,
            enabled,
            disabled,
            faulted,
        }
    }

    pub fn get_enabled_plugin(&self, plugin_id: &str) -> Option<RegisteredPlugin> {
        self.items
            .get(plugin_id)
            .filter(|plugin| plugin.metadata.status == PluginStatus::Enabled)
            .cloned()
    }

    pub fn get_plugin(&self, plugin_id: &str) -> Option<RegisteredPlugin> {
        self.items.get(plugin_id).cloned()
    }

    pub async fn install_from_directory(
        &mut self,
        source_dir: &Path,
        plugins_root: &Path,
    ) -> Result<String> {
        let manifest_path = source_dir.join("manifest.json");
        let content = tokio::fs::read_to_string(&manifest_path).await?;
        let manifest: PluginManifest = serde_json::from_str(&content)?;
        manifest.validate()?;

        let target_dir = plugins_root.join(&manifest.id);
        if target_dir.exists() {
            tokio::fs::remove_dir_all(&target_dir).await?;
        }
        copy_dir_recursive(source_dir, &target_dir).await?;

        let now = Utc::now();
        let installed_id = manifest.id.clone();
        self.items.insert(
            manifest.id.clone(),
            RegisteredPlugin {
                metadata: PluginMetadata {
                    plugin_id: manifest.id.clone(),
                    version: manifest.version.clone(),
                    install_path: target_dir,
                    status: PluginStatus::Enabled,
                    permissions: manifest.permissions.clone(),
                    checksum: None,
                    installed_at: now,
                    updated_at: now,
                },
                manifest,
            },
        );
        self.persist_metadata().await?;
        Ok(installed_id)
    }

    pub async fn set_status(&mut self, plugin_id: &str, status: PluginStatus) -> Result<()> {
        if let Some(plugin) = self.items.get_mut(plugin_id) {
            plugin.metadata.status = status;
            plugin.metadata.updated_at = Utc::now();
            self.persist_metadata().await?;
        }
        Ok(())
    }

    pub async fn uninstall(&mut self, plugin_id: &str) -> Result<()> {
        if let Some(plugin) = self.items.remove(plugin_id) {
            if plugin.metadata.install_path.exists() {
                tokio::fs::remove_dir_all(plugin.metadata.install_path).await?;
            }
            self.persist_metadata().await?;
        }
        Ok(())
    }

    async fn persist_metadata(&self) -> Result<()> {
        let items: Vec<PluginMetadata> = self
            .items
            .values()
            .map(|item| item.metadata.clone())
            .collect();
        self.metadata_store.save(&items).await
    }
}

#[cfg(test)]
mod tests {
    use super::{PluginRegistry, PluginStatus};
    use crate::storage::JsonStore;
    use anyhow::Result;
    use tempfile::TempDir;

    #[tokio::test]
    async fn can_scan_and_toggle_plugin_status() -> Result<()> {
        let temp = TempDir::new()?;
        let plugins_dir = temp.path().join("plugins");
        tokio::fs::create_dir_all(plugins_dir.join("demo")).await?;
        tokio::fs::write(
            plugins_dir.join("demo").join("manifest.json"),
            r#"{
              "id": "demo",
              "name": "Demo",
              "version": "0.1.0",
              "entry": "index.html",
              "commands": []
            }"#,
        )
        .await?;

        let store = JsonStore::new(temp.path().join("registry.json"));
        let mut registry = PluginRegistry::scan(&plugins_dir, store).await?;
        assert_eq!(registry.summary().enabled, 1);
        registry.set_status("demo", PluginStatus::Disabled).await?;
        assert_eq!(registry.summary().disabled, 1);
        Ok(())
    }
}
