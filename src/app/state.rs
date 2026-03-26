use crate::app::settings::UserSettings;
use crate::capability::{CapabilityAction, CapabilityGateway};
use crate::plugin::manifest::CommandMode;
use crate::plugin::registry::{PluginRegistry, PluginRegistrySummary, PluginStatus};
use crate::search::source::{BuiltinCommandSource, RecentSource, StaticCommandSource};
use crate::search::{SearchAction, SearchEngine, SearchItem, SearchRequest};
use crate::storage::{AppPaths, JsonStore, RecentEntry, copy_dir_recursive, ensure_dir};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct DesktopPlatform {
    pub paths: AppPaths,
    pub settings: UserSettings,
    pub plugin_registry: PluginRegistry,
    pub capability_gateway: CapabilityGateway,
    search_engine: SearchEngine,
    settings_store: JsonStore<UserSettings>,
    recent_store: JsonStore<Vec<RecentEntry>>,
    removed_builtin_store: JsonStore<Vec<String>>,
}

pub struct StartupReport {
    pub root_dir: PathBuf,
    pub loaded_plugins: PluginRegistrySummary,
    pub hotkey: String,
}

impl DesktopPlatform {
    pub async fn bootstrap(root_dir: impl AsRef<Path>) -> Result<Self> {
        let paths = AppPaths::new(root_dir);
        ensure_dir(&paths.root_dir).await?;
        ensure_dir(&paths.plugins_dir).await?;
        ensure_dir(&paths.data_dir).await?;
        seed_builtin_plugins(&paths).await?;

        let settings_store = JsonStore::new(paths.settings_file.clone());
        let recent_store = JsonStore::new(paths.recent_file.clone());
        let registry_store = JsonStore::new(paths.plugin_registry_file.clone());
        let removed_builtin_store = JsonStore::new(paths.removed_builtin_file.clone());

        let settings: UserSettings = settings_store.load_or_default().await?;
        let plugin_registry = PluginRegistry::scan(&paths.plugins_dir, registry_store).await?;
        let capability_gateway = CapabilityGateway;

        let mut search_engine = SearchEngine::new(
            settings.search_preferences.max_results,
            paths.search_timeout_ms,
        );
        search_engine.register_source(Box::new(BuiltinCommandSource::default()));
        search_engine.register_source(Box::new(StaticCommandSource::from_registry(
            plugin_registry.enabled_plugins(),
        )));

        if settings.search_preferences.include_recent {
            let recent_entries: Vec<RecentEntry> = recent_store.load_or_default().await?;
            search_engine.register_source(Box::new(RecentSource::new(recent_entries)));
        }

        Ok(Self {
            paths,
            settings,
            plugin_registry,
            capability_gateway,
            search_engine,
            settings_store,
            recent_store,
            removed_builtin_store,
        })
    }

    pub fn startup_report(&self) -> StartupReport {
        StartupReport {
            root_dir: self.paths.root_dir.clone(),
            loaded_plugins: self.plugin_registry.summary(),
            hotkey: self.settings.hotkey.clone(),
        }
    }

    pub async fn search(&self, query: impl Into<String>) -> Vec<SearchItem> {
        self.search_engine
            .search(SearchRequest {
                query: query.into(),
                limit: self.settings.search_preferences.max_results,
            })
            .await
    }

    pub fn plugin_display_name(&self, plugin_id: &str) -> Option<String> {
        self.plugin_registry
            .get_enabled_plugin(plugin_id)
            .map(|plugin| plugin.manifest.name)
    }

    pub fn plugin_entry_path(&self, plugin_id: &str) -> Option<PathBuf> {
        self.plugin_registry
            .get_enabled_plugin(plugin_id)
            .map(|plugin| plugin.metadata.install_path.join(plugin.manifest.entry))
    }

    pub fn search_in_plugin(&self, plugin_id: &str, query: impl Into<String>) -> Vec<SearchItem> {
        let query = query.into();
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        let Some(plugin) = self.plugin_registry.get_enabled_plugin(plugin_id) else {
            return Vec::new();
        };

        let mut items = Vec::new();
        for command in &plugin.manifest.commands {
            let mut keywords = plugin.manifest.keywords.clone();
            keywords.extend(command.keywords.clone());
            let subtitle = if command.description.is_empty() {
                plugin.manifest.name.clone()
            } else {
                format!("{} · {}", plugin.manifest.name, command.description)
            };

            let Some(score) = score_plugin_query(&query, &command.title, &subtitle, &keywords) else {
                continue;
            };

            items.push(SearchItem {
                source_type: "plugin_command".into(),
                source_id: command.id.clone(),
                plugin_id: Some(plugin.manifest.id.clone()),
                title: command.title.clone(),
                subtitle,
                keywords,
                score,
                action: SearchAction::PluginCommand {
                    plugin_id: plugin.manifest.id.clone(),
                    command_id: command.id.clone(),
                    mode: command.mode.clone(),
                },
            });
        }

        if items.is_empty() {
            if let Some(search_command) = plugin
                .manifest
                .commands
                .iter()
                .find(|command| matches!(command.mode, crate::plugin::manifest::CommandMode::Search))
            {
                let fallback_action = search_fallback_action(
                    &plugin.manifest.id,
                    &search_command.id,
                    &search_command.mode,
                    &query,
                );
                items.push(SearchItem {
                    source_type: "plugin_command".into(),
                    source_id: search_command.id.clone(),
                    plugin_id: Some(plugin.manifest.id.clone()),
                    title: format!("在 {} 中搜索 \"{}\"", plugin.manifest.name, query),
                    subtitle: "插件内搜索".into(),
                    keywords: vec![query.clone()],
                    score: 80.0,
                    action: fallback_action,
                });
            }
        }

        items.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.title.cmp(&right.title))
        });
        items.truncate(self.settings.search_preferences.max_results);
        items
    }

    pub async fn execute(&mut self, item: &SearchItem) -> Result<String> {
        let result = match &item.action {
            SearchAction::HostCommand(command) => match command.as_str() {
                "host.open_settings" => "打开设置页".to_string(),
                "host.reload_plugins" => {
                    self.reload_plugins().await?;
                    "已重载插件索引".to_string()
                }
                _ => format!("执行宿主命令: {command}"),
            },
            SearchAction::PluginCommand {
                plugin_id,
                command_id,
                mode,
            } => {
                let capability = CapabilityAction::RunPluginCommand {
                    plugin_id: plugin_id.clone(),
                    command_id: command_id.clone(),
                    mode: mode.clone(),
                };
                self.capability_gateway.handle(&capability).await?
            }
            SearchAction::OpenUrl(url) => {
                self.capability_gateway
                    .handle(&CapabilityAction::OpenUrl(url.clone()))
                    .await?
            }
            SearchAction::CopyText(text) => {
                self.capability_gateway
                    .handle(&CapabilityAction::CopyText(text.clone()))
                    .await?
            }
        };

        self.record_recent(item).await?;
        Ok(result)
    }

    pub async fn save_settings(&self) -> Result<()> {
        self.settings_store.save(&self.settings).await
    }

    pub async fn reload_plugins(&mut self) -> Result<()> {
        let registry_store = JsonStore::new(self.paths.plugin_registry_file.clone());
        self.plugin_registry =
            PluginRegistry::scan(&self.paths.plugins_dir, registry_store).await?;
        self.rebuild_search_engine().await?;
        Ok(())
    }

    pub async fn set_plugin_enabled(&mut self, plugin_id: &str, enabled: bool) -> Result<()> {
        let status = if enabled {
            PluginStatus::Enabled
        } else {
            PluginStatus::Disabled
        };
        self.plugin_registry.set_status(plugin_id, status).await?;
        self.rebuild_search_engine().await?;
        Ok(())
    }

    pub async fn uninstall_plugin(&mut self, plugin_id: &str) -> Result<()> {
        self.plugin_registry.uninstall(plugin_id).await?;
        self.mark_builtin_removed(plugin_id).await?;
        self.rebuild_search_engine().await?;
        Ok(())
    }

    pub async fn install_plugin_from_directory(&mut self, source_dir: &Path) -> Result<String> {
        let installed_id = self
            .plugin_registry
            .install_from_directory(source_dir, &self.paths.plugins_dir)
            .await?;
        self.unmark_builtin_removed(&installed_id).await?;
        self.rebuild_search_engine().await?;
        Ok(installed_id)
    }

    async fn rebuild_search_engine(&mut self) -> Result<()> {
        let mut search_engine = SearchEngine::new(
            self.settings.search_preferences.max_results,
            self.paths.search_timeout_ms,
        );
        search_engine.register_source(Box::new(BuiltinCommandSource::default()));
        search_engine.register_source(Box::new(StaticCommandSource::from_registry(
            self.plugin_registry.enabled_plugins(),
        )));
        if self.settings.search_preferences.include_recent {
            let recent_entries: Vec<RecentEntry> = self.recent_store.load_or_default().await?;
            search_engine.register_source(Box::new(RecentSource::new(recent_entries)));
        }
        self.search_engine = search_engine;
        Ok(())
    }

    async fn record_recent(&mut self, item: &SearchItem) -> Result<()> {
        let mut entries: Vec<RecentEntry> = self.recent_store.load_or_default().await?;
        entries.retain(|entry| entry.key != item.key());
        entries.insert(0, RecentEntry::from_search_item(item));
        entries.truncate(20);
        self.recent_store.save(&entries).await?;
        if self.settings.search_preferences.include_recent {
            self.rebuild_search_engine().await?;
        }
        Ok(())
    }

    async fn mark_builtin_removed(&self, plugin_id: &str) -> Result<()> {
        let builtin = builtin_root()
            .map(|root| root.join(plugin_id).exists())
            .unwrap_or(false);
        if !builtin {
            return Ok(());
        }

        let mut removed = self.removed_builtin_store.load_or_default().await?;
        if removed.iter().any(|item| item == plugin_id) {
            return Ok(());
        }
        removed.push(plugin_id.to_string());
        self.removed_builtin_store.save(&removed).await
    }

    async fn unmark_builtin_removed(&self, plugin_id: &str) -> Result<()> {
        let mut removed = self.removed_builtin_store.load_or_default().await?;
        let before = removed.len();
        removed.retain(|item| item != plugin_id);
        if removed.len() == before {
            return Ok(());
        }
        self.removed_builtin_store.save(&removed).await
    }
}

fn score_plugin_query(query: &str, title: &str, subtitle: &str, keywords: &[String]) -> Option<f32> {
    let title = title.to_lowercase();
    let subtitle = subtitle.to_lowercase();
    let keywords: Vec<String> = keywords.iter().map(|keyword| keyword.to_lowercase()).collect();

    if title == query {
        return Some(100.0);
    }
    if title.starts_with(query) {
        return Some(94.0);
    }
    if title.contains(query) {
        return Some(88.0);
    }
    if keywords.iter().any(|keyword| keyword == query) {
        return Some(85.0);
    }
    if keywords.iter().any(|keyword| keyword.contains(query)) {
        return Some(80.0);
    }
    if subtitle.contains(query) {
        return Some(72.0);
    }
    None
}

fn search_fallback_action(
    plugin_id: &str,
    command_id: &str,
    mode: &CommandMode,
    query: &str,
) -> SearchAction {
    if plugin_id == "web-search" {
        return SearchAction::OpenUrl(web_search_target(query));
    }
    if plugin_id == "file-search" {
        let encoded = urlencoding::encode(query);
        return SearchAction::OpenUrl(format!("search-ms:query={encoded}"));
    }

    SearchAction::PluginCommand {
        plugin_id: plugin_id.to_string(),
        command_id: command_id.to_string(),
        mode: mode.clone(),
    }
}

fn web_search_target(query: &str) -> String {
    let value = query.trim();
    if value.starts_with("http://") || value.starts_with("https://") {
        return value.to_string();
    }
    let encoded = urlencoding::encode(value);
    format!("https://www.baidu.com/s?wd={encoded}")
}

async fn seed_builtin_plugins(paths: &AppPaths) -> Result<()> {
    let removed_store = JsonStore::<Vec<String>>::new(paths.removed_builtin_file.clone());
    let removed: HashSet<String> = removed_store
        .load_or_default()
        .await?
        .into_iter()
        .collect();

    let builtin_root = builtin_root().filter(|path| path.exists());
    let Some(builtin_root) = builtin_root else {
        return Ok(());
    };

    let mut entries = tokio::fs::read_dir(builtin_root).await?;
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }

        let plugin_id = entry.file_name().to_string_lossy().to_string();
        let target_dir = paths.plugins_dir.join(&plugin_id);
        if removed.contains(&plugin_id) {
            if target_dir.exists() {
                tokio::fs::remove_dir_all(&target_dir).await?;
            }
            continue;
        }
        if target_dir.exists() {
            tokio::fs::remove_dir_all(&target_dir).await?;
        }
        copy_dir_recursive(entry.path(), &target_dir).await?;
    }
    Ok(())
}

fn builtin_root() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("plugins")
            .join("builtin"),
        std::env::current_dir()
            .ok()?
            .join("plugins")
            .join("builtin"),
        std::env::current_dir()
            .ok()?
            .parent()
            .map(|path| path.join("plugins").join("builtin"))?,
    ];

    candidates.into_iter().find(|path| path.exists())
}

#[cfg(test)]
mod tests {
    use super::DesktopPlatform;
    use anyhow::Result;
    use tempfile::TempDir;

    #[tokio::test]
    async fn bootstrap_creates_default_files() -> Result<()> {
        let temp = TempDir::new()?;
        let platform = DesktopPlatform::bootstrap(temp.path()).await?;
        let report = platform.startup_report();
        assert_eq!(report.hotkey, "Alt+Space");
        assert_eq!(report.loaded_plugins.total, 4);
        Ok(())
    }
}
