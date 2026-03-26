pub mod source;

use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchAction {
    HostCommand(String),
    PluginCommand {
        plugin_id: String,
        command_id: String,
        mode: crate::plugin::manifest::CommandMode,
    },
    OpenUrl(String),
    CopyText(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchItem {
    pub source_type: String,
    pub source_id: String,
    pub plugin_id: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub keywords: Vec<String>,
    pub score: f32,
    pub action: SearchAction,
}

impl SearchItem {
    pub fn key(&self) -> String {
        match &self.plugin_id {
            Some(plugin_id) => format!("{}:{}:{}", self.source_type, plugin_id, self.source_id),
            None => format!("{}:{}", self.source_type, self.source_id),
        }
    }
}

pub struct SearchEngine {
    sources: Vec<Box<dyn source::SearchSource>>,
    default_limit: usize,
    timeout: Duration,
}

impl SearchEngine {
    pub fn new(default_limit: usize, timeout_ms: u64) -> Self {
        Self {
            sources: Vec::new(),
            default_limit,
            timeout: Duration::from_millis(timeout_ms),
        }
    }

    pub fn register_source(&mut self, source: Box<dyn source::SearchSource>) {
        self.sources.push(source);
    }

    pub async fn search(&self, request: SearchRequest) -> Vec<SearchItem> {
        let limit = if request.limit == 0 {
            self.default_limit
        } else {
            request.limit
        };

        let futures = self.sources.iter().map(|source| {
            let source = source.as_ref();
            let request = request.clone();
            async move {
                tokio::time::timeout(self.timeout, source.search(&request))
                    .await
                    .ok()
            }
        });

        let mut items: Vec<SearchItem> = join_all(futures)
            .await
            .into_iter()
            .flatten()
            .flatten()
            .collect();

        let mut deduped: HashMap<String, SearchItem> = HashMap::new();
        for item in items.drain(..) {
            let key = item.key();
            match deduped.get_mut(&key) {
                Some(existing) if item.score > existing.score => {
                    *existing = item;
                }
                None => {
                    deduped.insert(key, item);
                }
                _ => {}
            }
        }
        items = deduped.into_values().collect();

        items.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.title.cmp(&right.title))
        });
        items.truncate(limit);
        items
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchAction, SearchEngine, SearchItem, SearchRequest, source::SearchSource};
    use async_trait::async_trait;

    struct MockSource {
        items: Vec<SearchItem>,
    }

    #[async_trait]
    impl SearchSource for MockSource {
        async fn search(&self, _request: &SearchRequest) -> Vec<SearchItem> {
            self.items.clone()
        }
    }

    #[tokio::test]
    async fn deduplicates_same_item_key_and_keeps_higher_score() {
        let item_low = SearchItem {
            source_type: "plugin_command".into(),
            source_id: "copy-template".into(),
            plugin_id: Some("clipboard-tools".into()),
            title: "复制常用文本".into(),
            subtitle: "Clipboard Tools".into(),
            keywords: vec!["copy".into()],
            score: 72.0,
            action: SearchAction::CopyText("demo".into()),
        };

        let mut item_high = item_low.clone();
        item_high.score = 99.0;

        let mut engine = SearchEngine::new(8, 500);
        engine.register_source(Box::new(MockSource {
            items: vec![item_low],
        }));
        engine.register_source(Box::new(MockSource {
            items: vec![item_high],
        }));

        let results = engine
            .search(SearchRequest {
                query: "复制".into(),
                limit: 8,
            })
            .await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score, 99.0);
    }
}
