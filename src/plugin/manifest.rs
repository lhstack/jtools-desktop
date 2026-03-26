use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommandMode {
    Action,
    View,
    Search,
}

impl Display for CommandMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Action => "action",
            Self::View => "view",
            Self::Search => "search",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    pub mode: CommandMode,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub entry: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub commands: Vec<PluginCommand>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub min_app_version: Option<String>,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("plugin id must not be empty");
        }
        if self.name.trim().is_empty() {
            bail!("plugin name must not be empty");
        }
        if self.version.trim().is_empty() {
            bail!("plugin version must not be empty");
        }
        if self.entry.trim().is_empty() {
            bail!("plugin entry must not be empty");
        }

        let mut seen = std::collections::HashSet::new();
        for command in &self.commands {
            if command.id.trim().is_empty() {
                bail!("plugin command id must not be empty");
            }
            if !seen.insert(command.id.clone()) {
                bail!(
                    "duplicate command id `{}` in plugin `{}`",
                    command.id,
                    self.id
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CommandMode, PluginCommand, PluginManifest};
    use anyhow::Result;

    #[test]
    fn validates_manifest() -> Result<()> {
        let manifest = PluginManifest {
            id: "demo".into(),
            name: "Demo".into(),
            version: "0.1.0".into(),
            description: String::new(),
            author: String::new(),
            entry: "index.html".into(),
            icon: None,
            permissions: vec![],
            commands: vec![PluginCommand {
                id: "run".into(),
                title: "Run".into(),
                mode: CommandMode::Action,
                keywords: vec!["demo".into()],
                description: String::new(),
                icon: None,
            }],
            keywords: vec![],
            platforms: vec![],
            min_app_version: None,
        };
        manifest.validate()?;
        Ok(())
    }
}
