use crate::plugin::manifest::PluginCommand;
use crate::plugin::registry::RegisteredPlugin;
use crate::search::{SearchAction, SearchItem, SearchRequest};
use crate::storage::RecentEntry;
use async_trait::async_trait;

#[async_trait]
pub trait SearchSource: Send + Sync {
    async fn search(&self, request: &SearchRequest) -> Vec<SearchItem>;
}

pub struct BuiltinCommandSource {
    commands: Vec<SearchItem>,
}

impl BuiltinCommandSource {
    fn default_commands() -> Vec<SearchItem> {
        vec![
            SearchItem {
                source_type: "host_command".into(),
                source_id: "host.open_settings".into(),
                plugin_id: None,
                title: "打开设置".into(),
                subtitle: "查看快捷键、主题和插件偏好".into(),
                keywords: vec!["setting".into(), "设置".into(), "preferences".into()],
                score: 0.0,
                action: SearchAction::HostCommand("host.open_settings".into()),
            },
        ]
    }
}

impl Default for BuiltinCommandSource {
    fn default() -> Self {
        Self {
            commands: Self::default_commands(),
        }
    }
}

#[async_trait]
impl SearchSource for BuiltinCommandSource {
    async fn search(&self, request: &SearchRequest) -> Vec<SearchItem> {
        self.commands
            .iter()
            .filter_map(|item| scored_item(item, &request.query))
            .collect()
    }
}

pub struct StaticCommandSource {
    items: Vec<SearchItem>,
}

impl StaticCommandSource {
    pub fn from_registry(plugins: Vec<RegisteredPlugin>) -> Self {
        let items = plugins
            .into_iter()
            .flat_map(|plugin| {
                let plugin_title = plugin.manifest.name.clone();
                let plugin_id = plugin.manifest.id.clone();
                let manifest_keywords = plugin.manifest.keywords.clone();
                plugin.manifest.commands.into_iter().map(move |command| {
                    plugin_command_to_search_item(
                        &plugin_id,
                        &plugin_title,
                        &manifest_keywords,
                        &command,
                    )
                })
            })
            .collect();
        Self { items }
    }
}

#[async_trait]
impl SearchSource for StaticCommandSource {
    async fn search(&self, request: &SearchRequest) -> Vec<SearchItem> {
        self.items
            .iter()
            .filter_map(|item| scored_item(item, &request.query))
            .collect()
    }
}

pub struct RecentSource {
    entries: Vec<RecentEntry>,
}

impl RecentSource {
    pub fn new(entries: Vec<RecentEntry>) -> Self {
        Self { entries }
    }
}

#[async_trait]
impl SearchSource for RecentSource {
    async fn search(&self, request: &SearchRequest) -> Vec<SearchItem> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let mut item = entry.item.clone();
                let base_score = if request.query.trim().is_empty() {
                    95.0
                } else {
                    score_text(&request.query, &item.title, &item.subtitle, &item.keywords)?
                };
                item.score = base_score + 5.0;
                Some(item)
            })
            .collect()
    }
}

fn plugin_command_to_search_item(
    plugin_id: &str,
    plugin_title: &str,
    manifest_keywords: &[String],
    command: &PluginCommand,
) -> SearchItem {
    let mut keywords = manifest_keywords.to_vec();
    keywords.extend(command.keywords.clone());
    SearchItem {
        source_type: "plugin_command".into(),
        source_id: command.id.clone(),
        plugin_id: Some(plugin_id.to_string()),
        title: command.title.clone(),
        subtitle: format!("{plugin_title} · {}", command.description),
        keywords,
        score: 0.0,
        action: SearchAction::PluginCommand {
            plugin_id: plugin_id.to_string(),
            command_id: command.id.clone(),
            mode: command.mode.clone(),
        },
    }
}

fn scored_item(item: &SearchItem, query: &str) -> Option<SearchItem> {
    let score = if query.trim().is_empty() {
        default_score(item)
    } else {
        score_text(query, &item.title, &item.subtitle, &item.keywords)?
    };

    let mut scored = item.clone();
    scored.score = score;
    Some(scored)
}

fn default_score(item: &SearchItem) -> f32 {
    match item.action {
        SearchAction::HostCommand(_) => 50.0,
        SearchAction::PluginCommand { .. } => 70.0,
        SearchAction::OpenUrl(_) => 60.0,
        SearchAction::CopyText(_) => 60.0,
    }
}

fn score_text(query: &str, title: &str, subtitle: &str, keywords: &[String]) -> Option<f32> {
    let query = query.trim().to_lowercase();
    let title_lower = title.to_lowercase();
    let subtitle_lower = subtitle.to_lowercase();
    let keywords_lower: Vec<String> = keywords
        .iter()
        .map(|keyword| keyword.to_lowercase())
        .collect();

    if title_lower == query {
        return Some(100.0);
    }
    if title_lower.starts_with(&query) {
        return Some(92.0);
    }
    if keywords_lower.iter().any(|keyword| keyword == &query) {
        return Some(90.0);
    }
    if title_lower.contains(&query) {
        return Some(85.0);
    }
    if keywords_lower
        .iter()
        .any(|keyword| keyword.contains(&query))
    {
        return Some(78.0);
    }
    if subtitle_lower.contains(&query) {
        return Some(72.0);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{RecentSource, SearchSource, StaticCommandSource};
    use crate::plugin::manifest::{CommandMode, PluginCommand, PluginManifest, PluginUiConfig};
    use crate::plugin::registry::{PluginMetadata, PluginStatus, RegisteredPlugin};
    use crate::storage::RecentEntry;
    use anyhow::Result;
    use chrono::Utc;
    use std::path::PathBuf;

    #[tokio::test]
    async fn static_command_source_matches_keywords() -> Result<()> {
        let plugin = RegisteredPlugin {
            manifest: PluginManifest {
                id: "web-search".into(),
                name: "Web Search".into(),
                version: "0.1.0".into(),
                description: String::new(),
                author: String::new(),
                entry: "index.html".into(),
                icon: None,
                permissions: vec![],
                commands: vec![PluginCommand {
                    id: "search".into(),
                    title: "网页搜索".into(),
                    mode: CommandMode::Search,
                    keywords: vec!["google".into(), "baidu".into()],
                    description: "按关键词发起网页搜索".into(),
                    icon: None,
                }],
                keywords: vec!["web".into()],
                platforms: vec![],
                min_app_version: None,
                ui: PluginUiConfig::default(),
            },
            metadata: PluginMetadata {
                plugin_id: "web-search".into(),
                version: "0.1.0".into(),
                install_path: PathBuf::from("plugins/web-search"),
                status: PluginStatus::Enabled,
                permissions: vec![],
                checksum: None,
                installed_at: Utc::now(),
                updated_at: Utc::now(),
            },
        };

        let source = StaticCommandSource::from_registry(vec![plugin]);
        let items = source
            .search(&crate::search::SearchRequest {
                query: "google".into(),
                limit: 10,
            })
            .await;
        assert_eq!(items.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn recent_source_boosts_empty_query() -> Result<()> {
        let item = crate::search::SearchItem {
            source_type: "plugin_command".into(),
            source_id: "copy".into(),
            plugin_id: Some("clipboard".into()),
            title: "复制剪贴板".into(),
            subtitle: "Clipboard Tools".into(),
            keywords: vec!["copy".into()],
            score: 0.0,
            action: crate::search::SearchAction::CopyText("demo".into()),
        };
        let source = RecentSource::new(vec![RecentEntry::from_search_item(&item)]);
        let items = source
            .search(&crate::search::SearchRequest {
                query: String::new(),
                limit: 10,
            })
            .await;
        assert_eq!(items[0].score, 100.0);
        Ok(())
    }
}
