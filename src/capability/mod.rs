use crate::plugin::manifest::CommandMode;
use anyhow::Result;

#[derive(Debug, Clone)]
pub enum CapabilityAction {
    OpenUrl(String),
    CopyText(String),
    RunPluginCommand {
        plugin_id: String,
        command_id: String,
        mode: CommandMode,
    },
}

#[derive(Debug, Default)]
pub struct CapabilityGateway;

impl CapabilityGateway {
    pub async fn handle(&self, action: &CapabilityAction) -> Result<String> {
        let message = match action {
            CapabilityAction::OpenUrl(url) => {
                open::that_detached(url)?;
                format!("已打开: {url}")
            }
            CapabilityAction::CopyText(text) => {
                let mut clipboard = arboard::Clipboard::new()?;
                clipboard.set_text(text.clone())?;
                "已复制到剪贴板".to_string()
            }
            CapabilityAction::RunPluginCommand {
                plugin_id,
                command_id,
                mode,
            } => format!("已执行插件命令 {plugin_id}/{command_id} ({mode})"),
        };
        Ok(message)
    }
}
