use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowBehavior {
    pub hide_on_blur: bool,
    pub close_to_tray: bool,
}

impl Default for WindowBehavior {
    fn default() -> Self {
        Self {
            hide_on_blur: true,
            close_to_tray: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchPreferences {
    pub max_results: usize,
    pub include_recent: bool,
}

impl Default for SearchPreferences {
    fn default() -> Self {
        Self {
            max_results: 8,
            include_recent: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserSettings {
    pub theme: String,
    pub language: String,
    pub hotkey: String,
    pub window_behavior: WindowBehavior,
    pub plugin_preferences: HashMap<String, bool>,
    pub search_preferences: SearchPreferences,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            language: "zh-CN".to_string(),
            hotkey: "Alt+Space".to_string(),
            window_behavior: WindowBehavior::default(),
            plugin_preferences: HashMap::new(),
            search_preferences: SearchPreferences::default(),
        }
    }
}
