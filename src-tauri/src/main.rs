#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use jtools::app::state::DesktopPlatform;
use jtools::plugin::registry::PluginStatus;
use jtools::search::{SearchAction, SearchItem};
use base64::Engine;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, State, Url, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;
use tokio::sync::Mutex;
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

struct AppState {
    platform: Mutex<DesktopPlatform>,
}

async fn run_native_dialog<T, F>(app: &tauri::AppHandle, dialog_task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let window = app.get_webview_window("main");

    let result = tauri::async_runtime::spawn_blocking(dialog_task)
        .await
        .map_err(|error| error.to_string());

    if let Some(main) = window.as_ref() {
        let _ = main.set_focus();
    }

    result
}

fn preferences_from_platform(platform: &DesktopPlatform) -> UserPreferences {
    UserPreferences {
        theme: platform.settings.theme.clone(),
        language: platform.settings.language.clone(),
        hotkey: platform.settings.hotkey.clone(),
        max_results: platform.settings.search_preferences.max_results,
        include_recent: platform.settings.search_preferences.include_recent,
        hide_on_blur: platform.settings.window_behavior.hide_on_blur,
        close_to_tray: platform.settings.window_behavior.close_to_tray,
        root_dir: platform.paths.root_dir.display().to_string(),
        plugins_dir: platform.paths.plugins_dir.display().to_string(),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    hotkey: String,
    root_dir: String,
    total_plugins: usize,
    enabled_plugins: usize,
    disabled_plugins: usize,
    faulted_plugins: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginListItem {
    id: String,
    name: String,
    description: String,
    command_count: usize,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginScopeRequest {
    plugin_id: String,
    query: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginRefRequest {
    plugin_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginToggleRequest {
    plugin_id: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JtpImportRequest {
    file_name: String,
    data_base64: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JtpExportRequest {
    plugin_id: String,
    output_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateRequest {
    target_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginPackSourceRequest {
    source_dir: String,
    output_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserPreferences {
    theme: String,
    language: String,
    hotkey: String,
    max_results: usize,
    include_recent: bool,
    hide_on_blur: bool,
    close_to_tray: bool,
    root_dir: String,
    plugins_dir: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPreferencesUpdateRequest {
    theme: Option<String>,
    language: Option<String>,
    hotkey: Option<String>,
    max_results: Option<usize>,
    include_recent: Option<bool>,
    hide_on_blur: Option<bool>,
    close_to_tray: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManageCommandItem {
    id: String,
    title: String,
    subtitle: String,
    plugin_id: Option<String>,
    command_id: Option<String>,
    mode: Option<String>,
    enabled: bool,
    keywords: Vec<String>,
    action: SearchAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginCapabilityRequest {
    plugin_id: String,
    capability: String,
    request_id: Option<String>,
    #[serde(default)]
    args: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginCapabilityResponse {
    request_id: Option<String>,
    ok: bool,
    message: String,
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginViewOptions {
    show_search_input: bool,
}

#[tauri::command]
async fn get_app_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let platform = state.platform.lock().await;
    let report = platform.startup_report();
    Ok(AppStatus {
        hotkey: report.hotkey,
        root_dir: report.root_dir.display().to_string(),
        total_plugins: report.loaded_plugins.total,
        enabled_plugins: report.loaded_plugins.enabled,
        disabled_plugins: report.loaded_plugins.disabled,
        faulted_plugins: report.loaded_plugins.faulted,
    })
}

#[tauri::command]
async fn get_user_preferences(state: State<'_, AppState>) -> Result<UserPreferences, String> {
    let platform = state.platform.lock().await;
    Ok(preferences_from_platform(&platform))
}

#[tauri::command]
async fn update_user_preferences(
    state: State<'_, AppState>,
    payload: UserPreferencesUpdateRequest,
) -> Result<UserPreferences, String> {
    let mut platform = state.platform.lock().await;
    let mut need_refresh_search = false;

    if let Some(theme) = payload.theme {
        let value = theme.trim();
        if !value.is_empty() {
            platform.settings.theme = value.to_string();
        }
    }
    if let Some(language) = payload.language {
        let value = language.trim();
        if !value.is_empty() {
            platform.settings.language = value.to_string();
        }
    }
    if let Some(hotkey) = payload.hotkey {
        let value = hotkey.trim();
        if !value.is_empty() {
            platform.settings.hotkey = value.to_string();
        }
    }
    if let Some(max_results) = payload.max_results {
        let clamped = max_results.clamp(1, 50);
        if platform.settings.search_preferences.max_results != clamped {
            platform.settings.search_preferences.max_results = clamped;
            need_refresh_search = true;
        }
    }
    if let Some(include_recent) = payload.include_recent {
        if platform.settings.search_preferences.include_recent != include_recent {
            platform.settings.search_preferences.include_recent = include_recent;
            need_refresh_search = true;
        }
    }
    if let Some(hide_on_blur) = payload.hide_on_blur {
        platform.settings.window_behavior.hide_on_blur = hide_on_blur;
    }
    if let Some(close_to_tray) = payload.close_to_tray {
        platform.settings.window_behavior.close_to_tray = close_to_tray;
    }

    platform
        .save_settings()
        .await
        .map_err(|error| error.to_string())?;
    if need_refresh_search {
        platform
            .reload_plugins()
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok(preferences_from_platform(&platform))
}

#[tauri::command]
async fn list_manage_commands(state: State<'_, AppState>) -> Result<Vec<ManageCommandItem>, String> {
    let platform = state.platform.lock().await;
    let mut items = vec![
        ManageCommandItem {
            id: "host.open_settings".to_string(),
            title: "打开设置".to_string(),
            subtitle: "宿主命令".to_string(),
            plugin_id: None,
            command_id: Some("host.open_settings".to_string()),
            mode: Some("action".to_string()),
            enabled: true,
            keywords: vec!["setting".to_string(), "设置".to_string(), "preferences".to_string()],
            action: SearchAction::HostCommand("host.open_settings".to_string()),
        },
        ManageCommandItem {
            id: "host.reload_plugins".to_string(),
            title: "重载插件".to_string(),
            subtitle: "宿主命令".to_string(),
            plugin_id: None,
            command_id: Some("host.reload_plugins".to_string()),
            mode: Some("action".to_string()),
            enabled: true,
            keywords: vec!["reload".to_string(), "插件".to_string(), "plugin".to_string()],
            action: SearchAction::HostCommand("host.reload_plugins".to_string()),
        },
    ];

    for plugin in platform.plugin_registry.all_plugins() {
        let plugin_enabled = matches!(plugin.metadata.status, PluginStatus::Enabled);
        for command in &plugin.manifest.commands {
            let mut keywords = plugin.manifest.keywords.clone();
            keywords.extend(command.keywords.clone());
            let subtitle = if command.description.is_empty() {
                plugin.manifest.name.clone()
            } else {
                format!("{} · {}", plugin.manifest.name, command.description)
            };
            items.push(ManageCommandItem {
                id: format!("plugin:{}:{}", plugin.manifest.id, command.id),
                title: command.title.clone(),
                subtitle,
                plugin_id: Some(plugin.manifest.id.clone()),
                command_id: Some(command.id.clone()),
                mode: Some(command.mode.to_string()),
                enabled: plugin_enabled,
                keywords,
                action: SearchAction::PluginCommand {
                    plugin_id: plugin.manifest.id.clone(),
                    command_id: command.id.clone(),
                    mode: command.mode.clone(),
                },
            });
        }
    }

    items.sort_by(|left, right| {
        right
            .enabled
            .cmp(&left.enabled)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(items)
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: String) -> Result<Vec<SearchItem>, String> {
    let platform = state.platform.lock().await;
    Ok(platform.search(query).await)
}

#[tauri::command]
async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginListItem>, String> {
    let platform = state.platform.lock().await;
    let mut items: Vec<PluginListItem> = platform
        .plugin_registry
        .all_plugins()
        .into_iter()
        .map(|plugin| PluginListItem {
            id: plugin.manifest.id,
            name: plugin.manifest.name,
            description: plugin.manifest.description,
            command_count: plugin.manifest.commands.len(),
            enabled: matches!(plugin.metadata.status, PluginStatus::Enabled),
        })
        .collect();

    items.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(items)
}

#[tauri::command]
async fn set_plugin_enabled(
    state: State<'_, AppState>,
    payload: PluginToggleRequest,
) -> Result<(), String> {
    let mut platform = state.platform.lock().await;
    platform
        .set_plugin_enabled(&payload.plugin_id, payload.enabled)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn uninstall_plugin(
    state: State<'_, AppState>,
    payload: PluginRefRequest,
) -> Result<(), String> {
    let mut platform = state.platform.lock().await;
    platform
        .uninstall_plugin(&payload.plugin_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn install_plugin_from_jtp(
    state: State<'_, AppState>,
    payload: JtpImportRequest,
) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.data_base64.trim())
        .map_err(|error| format!("jtp decode failed: {error}"))?;
    install_plugin_from_jtp_bytes(&state, bytes, payload.file_name.trim()).await
}

#[tauri::command]
async fn install_plugin_from_jtp_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let selected = run_native_dialog(&app, || {
        FileDialog::new()
            .add_filter("jtools plugin package", &["jtp"])
            .pick_file()
    })
    .await?;
    let Some(file_path) = selected else {
        return Err("已取消选择文件".to_string());
    };

    let file_name = file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin.jtp")
        .to_string();
    let bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|error| error.to_string())?;
    install_plugin_from_jtp_bytes(&state, bytes, &file_name).await
}

async fn install_plugin_from_jtp_bytes(
    state: &State<'_, AppState>,
    bytes: Vec<u8>,
    label: &str,
) -> Result<String, String> {
    let marker = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let temp_root = std::env::temp_dir().join(format!(
        "jtools-jtp-{}-{}",
        std::process::id(),
        marker
    ));
    let extract_root = temp_root.join("extract");
    std::fs::create_dir_all(&extract_root).map_err(|error| error.to_string())?;

    let (plugin_root, plugin_id) = tauri::async_runtime::spawn_blocking({
        let extract_root = extract_root.clone();
        move || -> Result<(PathBuf, String), String> {
            unpack_jtp_bytes(&bytes, &extract_root).map_err(|error| error.to_string())?;
            let plugin_root = detect_plugin_root(&extract_root).map_err(|error| error.to_string())?;
            let plugin_id = read_plugin_id(&plugin_root).map_err(|error| error.to_string())?;
            Ok((plugin_root, plugin_id))
        }
    })
    .await
    .map_err(|error| error.to_string())??;

    let (plugins_root, removed_builtin_file) = {
        let platform = state.platform.lock().await;
        (
            platform.paths.plugins_dir.clone(),
            platform.paths.removed_builtin_file.clone(),
        )
    };

    tauri::async_runtime::spawn_blocking({
        let plugin_root = plugin_root.clone();
        let plugin_id = plugin_id.clone();
        move || -> Result<(), String> {
            let target_dir = plugins_root.join(&plugin_id);
            if target_dir.exists() {
                std::fs::remove_dir_all(&target_dir).map_err(|error| error.to_string())?;
            }
            copy_directory_blocking(&plugin_root, &target_dir).map_err(|error| error.to_string())?;
            remove_id_from_removed_builtin(&removed_builtin_file, &plugin_id)
                .map_err(|error| error.to_string())?;
            Ok(())
        }
    })
    .await
    .map_err(|error| error.to_string())??;

    {
        let mut platform = state.platform.lock().await;
        platform
            .reload_plugins()
            .await
            .map_err(|error| error.to_string())?;
    }

    let _ = std::fs::remove_dir_all(&temp_root);
    Ok(format!(
        "已导入 {}，插件ID={}",
        label,
        plugin_id
    ))
}

#[tauri::command]
async fn export_plugin_to_jtp(
    state: State<'_, AppState>,
    payload: JtpExportRequest,
) -> Result<String, String> {
    let (source_dir, file_name) = {
        let platform = state.platform.lock().await;
        let plugin = platform
            .plugin_registry
            .get_plugin(&payload.plugin_id)
            .ok_or_else(|| format!("plugin not found: {}", payload.plugin_id))?;
        let file_name = format!(
            "{}-{}.jtp",
            sanitize_path_segment(&plugin.manifest.id),
            sanitize_path_segment(&plugin.manifest.version)
        );
        (plugin.metadata.install_path, file_name)
    };

    let output_dir = PathBuf::from(payload.output_dir.trim());
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let output_path = output_dir.join(file_name);

    tauri::async_runtime::spawn_blocking({
        let output_path = output_path.clone();
        move || -> Result<(), String> {
            pack_directory_as_jtp(&source_dir, &output_path).map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())??;

    Ok(output_path.display().to_string())
}

#[tauri::command]
async fn pack_jtp_from_directory(payload: PluginPackSourceRequest) -> Result<String, String> {
    let source_dir = PathBuf::from(payload.source_dir.trim());
    let output_dir = PathBuf::from(payload.output_dir.trim());
    if !source_dir.join("manifest.json").exists() {
        return Err("选择的目录缺少 manifest.json".to_string());
    }
    if output_dir.as_os_str().is_empty() {
        return Err("输出目录不能为空".to_string());
    }

    let (plugin_id, plugin_version) =
        read_plugin_id_version(&source_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let output_path = output_dir.join(format!(
        "{}-{}.jtp",
        sanitize_path_segment(&plugin_id),
        sanitize_path_segment(&plugin_version)
    ));

    tauri::async_runtime::spawn_blocking({
        let source_dir = source_dir.clone();
        let output_path = output_path.clone();
        move || -> Result<(), String> {
            pack_directory_as_jtp(&source_dir, &output_path).map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(output_path.display().to_string())
}

#[tauri::command]
async fn pack_jtp_from_dialog(app: tauri::AppHandle) -> Result<String, String> {
    let selected_source = run_native_dialog(&app, || FileDialog::new().pick_folder()).await?;
    let Some(source_dir) = selected_source else {
        return Err("已取消选择打包目录".to_string());
    };

    let selected_output = run_native_dialog(&app, || FileDialog::new().pick_folder()).await?;
    let Some(output_dir) = selected_output else {
        return Err("已取消选择输出目录".to_string());
    };

    pack_jtp_from_directory(PluginPackSourceRequest {
        source_dir: source_dir.display().to_string(),
        output_dir: output_dir.display().to_string(),
    })
    .await
}

#[tauri::command]
async fn download_plugin_template(payload: TemplateRequest) -> Result<String, String> {
    let target_dir = PathBuf::from(payload.target_dir.trim());
    if target_dir.as_os_str().is_empty() {
        return Err("target_dir must not be empty".to_string());
    }

    let marker = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let template_dir = target_dir.join(format!("jtools-plugin-template-{marker}"));
    std::fs::create_dir_all(&template_dir).map_err(|error| error.to_string())?;
    write_plugin_template(&template_dir).map_err(|error| error.to_string())?;
    Ok(template_dir.display().to_string())
}

#[tauri::command]
async fn download_plugin_template_dialog(app: tauri::AppHandle) -> Result<String, String> {
    let selected = run_native_dialog(&app, || FileDialog::new().pick_folder()).await?;
    let Some(target_dir) = selected else {
        return Err("已取消选择模板保存目录".to_string());
    };

    download_plugin_template(TemplateRequest {
        target_dir: target_dir.display().to_string(),
    })
    .await
}

#[tauri::command]
async fn search_in_plugin(
    state: State<'_, AppState>,
    payload: PluginScopeRequest,
) -> Result<Vec<SearchItem>, String> {
    let platform = state.platform.lock().await;
    Ok(platform.search_in_plugin(&payload.plugin_id, payload.query))
}

#[tauri::command]
async fn plugin_display_name(
    state: State<'_, AppState>,
    payload: PluginRefRequest,
) -> Result<Option<String>, String> {
    let platform = state.platform.lock().await;
    Ok(platform.plugin_display_name(&payload.plugin_id))
}

#[tauri::command]
async fn plugin_view_options(
    state: State<'_, AppState>,
    payload: PluginRefRequest,
) -> Result<Option<PluginViewOptions>, String> {
    let platform = state.platform.lock().await;
    Ok(platform
        .plugin_show_search_input(&payload.plugin_id)
        .map(|show_search_input| PluginViewOptions { show_search_input }))
}

#[tauri::command]
async fn plugin_window_icon_bytes(
    state: State<'_, AppState>,
    payload: PluginRefRequest,
) -> Result<Option<Vec<u8>>, String> {
    let icon_path = {
        let platform = state.platform.lock().await;
        let plugin = platform
            .plugin_registry
            .get_enabled_plugin(&payload.plugin_id)
            .or_else(|| platform.plugin_registry.get_plugin(&payload.plugin_id));
        let Some(plugin) = plugin else {
            return Ok(None);
        };
        let icon_rel = plugin
            .manifest
            .icon
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(icon_rel) = icon_rel else {
            return Ok(None);
        };
        resolve_plugin_icon_path(&plugin.metadata.install_path, icon_rel)
    };

    let Some(path) = icon_path else {
        return Ok(None);
    };

    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| error.to_string())?;
    Ok(Some(bytes))
}

#[tauri::command]
async fn plugin_view_html(
    state: State<'_, AppState>,
    payload: PluginRefRequest,
) -> Result<Option<String>, String> {
    let entry_path = {
        let platform = state.platform.lock().await;
        platform.plugin_entry_path(&payload.plugin_id)
    };

    let Some(path) = entry_path else {
        return Ok(None);
    };

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => {
            let base_href = plugin_entry_base_href(&path);
            Ok(Some(inject_plugin_base_href(content, base_href.as_deref())))
        }
        Err(_) => Ok(None),
    }
}

#[tauri::command]
async fn capability_open_url(state: State<'_, AppState>, url: String) -> Result<String, String> {
    let platform = state.platform.lock().await;
    platform
        .capability_gateway
        .handle(&jtools::capability::CapabilityAction::OpenUrl(url))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn capability_copy_text(state: State<'_, AppState>, text: String) -> Result<String, String> {
    let platform = state.platform.lock().await;
    platform
        .capability_gateway
        .handle(&jtools::capability::CapabilityAction::CopyText(text))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn plugin_capability_call(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    payload: PluginCapabilityRequest,
) -> Result<PluginCapabilityResponse, String> {
    let plugin_id = payload.plugin_id.trim();
    if plugin_id.is_empty() {
        return Err("plugin_id 不能为空".to_string());
    }

    let (plugin_permissions, data_dir) = {
        let platform = state.platform.lock().await;
        let plugin = platform
            .plugin_registry
            .get_enabled_plugin(plugin_id)
            .ok_or_else(|| format!("插件不可用: {}", plugin_id))?;
        (plugin.manifest.permissions, platform.paths.data_dir.clone())
    };
    let files_root = plugin_files_root(&data_dir, plugin_id);
    let cache_file = plugin_cache_file(&data_dir, plugin_id);

    let capability = payload.capability.trim().to_lowercase();
    let required_permissions = match capability.as_str() {
        "open_url" => &["system.open_url", "network.http"][..],
        "copy_text" => &["clipboard.write"][..],
        "read_clipboard" => &["clipboard.read"][..],
        "hide_to_tray" | "show_launcher" => &["window.control"][..],
        "reload_plugins" => &["system.plugin.reload"][..],
        "exec_shell" => &["system.exec"][..],
        "file_read_path" => &["fs.system.read"][..],
        "file_write_path" | "file_append_path" | "file_remove_path" => &["fs.system.write"][..],
        "file_read_text" => &["fs.read"][..],
        "file_write_text" | "file_append_text" | "file_create_dir" => &["fs.write"][..],
        "file_list_dir" | "file_exists" => &["fs.list"][..],
        "file_remove" => &["fs.delete"][..],
        "cache_get" | "cache_list_keys" => &["cache.read"][..],
        "cache_set" | "cache_delete" | "cache_clear" => &["cache.write"][..],
        _ => return Err(format!("不支持的插件能力: {}", payload.capability)),
    };
    ensure_plugin_permission(&plugin_permissions, required_permissions)?;

    let message = match capability.as_str() {
        "open_url" => {
            let url = read_required_arg(&payload.args, "url")?;
            let platform = state.platform.lock().await;
            platform
                .capability_gateway
                .handle(&jtools::capability::CapabilityAction::OpenUrl(url))
                .await
                .map_err(|error| error.to_string())?
        }
        "copy_text" => {
            let text = read_required_arg(&payload.args, "text")?;
            let platform = state.platform.lock().await;
            platform
                .capability_gateway
                .handle(&jtools::capability::CapabilityAction::CopyText(text))
                .await
                .map_err(|error| error.to_string())?
        }
        "read_clipboard" => {
            let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
            let text = clipboard.get_text().map_err(|error| error.to_string())?;
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "读取剪贴板成功".to_string(),
                data: Some(serde_json::json!({ "text": text })),
            });
        }
        "hide_to_tray" => {
            hide_main_window(&app)?;
            "已隐藏到托盘".to_string()
        }
        "show_launcher" => {
            show_main_window(&app)?;
            "已显示启动器".to_string()
        }
        "reload_plugins" => {
            let mut platform = state.platform.lock().await;
            platform
                .reload_plugins()
                .await
                .map_err(|error| error.to_string())?;
            "插件目录已重载".to_string()
        }
        "exec_shell" => {
            let shell_command = read_required_arg(&payload.args, "command")?;
            let cwd = read_optional_trimmed_arg(&payload.args, "cwd");
            let output = execute_shell_command(&shell_command, cwd.as_deref())?;
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "命令执行完成".to_string(),
                data: Some(serde_json::json!(output)),
            });
        }
        "file_read_path" => {
            let path = read_required_arg(&payload.args, "path")?;
            let target = resolve_any_path(&path)?;
            let text = std::fs::read_to_string(&target).map_err(|error| error.to_string())?;
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "读取文件成功".to_string(),
                data: Some(serde_json::json!({
                    "path": target.display().to_string(),
                    "content": text
                })),
            });
        }
        "file_write_path" => {
            let path = read_required_arg(&payload.args, "path")?;
            let content = read_required_raw_string(&payload.args, "content")?;
            let target = resolve_any_path(&path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&target, content).map_err(|error| error.to_string())?;
            "写入系统路径成功".to_string()
        }
        "file_append_path" => {
            let path = read_required_arg(&payload.args, "path")?;
            let content = read_required_raw_string(&payload.args, "content")?;
            let target = resolve_any_path(&path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&target)
                .map_err(|error| error.to_string())?;
            file.write_all(content.as_bytes())
                .map_err(|error| error.to_string())?;
            "追加系统路径成功".to_string()
        }
        "file_remove_path" => {
            let path = read_required_arg(&payload.args, "path")?;
            let target = resolve_any_path(&path)?;
            if target.is_dir() {
                std::fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
            } else if target.exists() {
                std::fs::remove_file(&target).map_err(|error| error.to_string())?;
            }
            "删除系统路径成功".to_string()
        }
        "file_read_text" => {
            let path = read_required_arg(&payload.args, "path")?;
            let target = resolve_plugin_file_path(&files_root, &path)?;
            let text = std::fs::read_to_string(&target).map_err(|error| error.to_string())?;
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "读取文件成功".to_string(),
                data: Some(serde_json::json!({
                    "path": path,
                    "content": text
                })),
            });
        }
        "file_write_text" => {
            let path = read_required_arg(&payload.args, "path")?;
            let content = read_required_raw_string(&payload.args, "content")?;
            let target = resolve_plugin_file_path(&files_root, &path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&target, content).map_err(|error| error.to_string())?;
            "写入文件成功".to_string()
        }
        "file_append_text" => {
            let path = read_required_arg(&payload.args, "path")?;
            let content = read_required_raw_string(&payload.args, "content")?;
            let target = resolve_plugin_file_path(&files_root, &path)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&target)
                .map_err(|error| error.to_string())?;
            file.write_all(content.as_bytes())
                .map_err(|error| error.to_string())?;
            "追加文件成功".to_string()
        }
        "file_create_dir" => {
            let path = read_required_arg(&payload.args, "path")?;
            let target = resolve_plugin_file_path(&files_root, &path)?;
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            "目录创建成功".to_string()
        }
        "file_exists" => {
            let path = read_required_arg(&payload.args, "path")?;
            let target = resolve_plugin_file_path(&files_root, &path)?;
            let exists = target.exists();
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "查询完成".to_string(),
                data: Some(serde_json::json!({
                    "path": path,
                    "exists": exists
                })),
            });
        }
        "file_list_dir" => {
            let relative = read_optional_trimmed_arg(&payload.args, "path").unwrap_or_default();
            let target = if relative.is_empty() {
                files_root.clone()
            } else {
                resolve_plugin_file_path(&files_root, &relative)?
            };
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            let mut items = Vec::new();
            for entry in std::fs::read_dir(&target).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let meta = entry.metadata().map_err(|error| error.to_string())?;
                let name = entry.file_name().to_string_lossy().to_string();
                items.push(serde_json::json!({
                    "name": name,
                    "isDir": meta.is_dir(),
                    "size": if meta.is_file() { meta.len() } else { 0 },
                    "modifiedAt": meta
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map(|duration| duration.as_secs())
                }));
            }
            items.sort_by(|left, right| {
                let left_dir = left.get("isDir").and_then(|value| value.as_bool()).unwrap_or(false);
                let right_dir = right.get("isDir").and_then(|value| value.as_bool()).unwrap_or(false);
                right_dir
                    .cmp(&left_dir)
                    .then_with(|| {
                        left.get("name")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default()
                            .cmp(right.get("name").and_then(|value| value.as_str()).unwrap_or_default())
                    })
            });
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "目录读取成功".to_string(),
                data: Some(serde_json::json!({
                    "path": relative,
                    "items": items
                })),
            });
        }
        "file_remove" => {
            let path = read_required_arg(&payload.args, "path")?;
            let target = resolve_plugin_file_path(&files_root, &path)?;
            if target.is_dir() {
                std::fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
            } else if target.exists() {
                std::fs::remove_file(&target).map_err(|error| error.to_string())?;
            }
            "删除成功".to_string()
        }
        "cache_get" => {
            let key = read_required_arg(&payload.args, "key")?;
            let cache = load_plugin_cache(&cache_file)?;
            let value = cache.get(&key).cloned();
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "读取缓存成功".to_string(),
                data: Some(serde_json::json!({
                    "key": key,
                    "exists": value.is_some(),
                    "value": value
                })),
            });
        }
        "cache_set" => {
            let key = read_required_arg(&payload.args, "key")?;
            let value = payload.args.get("value").cloned().unwrap_or(Value::Null);
            let mut cache = load_plugin_cache(&cache_file)?;
            cache.insert(key, value);
            save_plugin_cache(&cache_file, &cache)?;
            "写入缓存成功".to_string()
        }
        "cache_delete" => {
            let key = read_required_arg(&payload.args, "key")?;
            let mut cache = load_plugin_cache(&cache_file)?;
            cache.remove(&key);
            save_plugin_cache(&cache_file, &cache)?;
            "删除缓存成功".to_string()
        }
        "cache_list_keys" => {
            let cache = load_plugin_cache(&cache_file)?;
            let mut keys: Vec<String> = cache.keys().cloned().collect();
            keys.sort();
            return Ok(PluginCapabilityResponse {
                request_id: payload.request_id,
                ok: true,
                message: "读取缓存键成功".to_string(),
                data: Some(serde_json::json!({
                    "keys": keys
                })),
            });
        }
        "cache_clear" => {
            save_plugin_cache(&cache_file, &serde_json::Map::new())?;
            "已清空缓存".to_string()
        }
        _ => return Err(format!("不支持的插件能力: {}", payload.capability)),
    };

    Ok(PluginCapabilityResponse {
        request_id: payload.request_id,
        ok: true,
        message,
        data: None,
    })
}

#[tauri::command]
async fn execute_item(state: State<'_, AppState>, item: SearchItem) -> Result<String, String> {
    let mut platform = state.platform.lock().await;
    platform
        .execute(&item)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn reload_plugins(state: State<'_, AppState>) -> Result<String, String> {
    let mut platform = state.platform.lock().await;
    platform
        .reload_plugins()
        .await
        .map_err(|error| error.to_string())?;
    Ok("插件目录已重载并刷新索引。".to_string())
}

#[tauri::command]
fn hide_launcher_to_tray(app: tauri::AppHandle) -> Result<(), String> {
    hide_main_window(&app)
}

#[tauri::command]
fn show_launcher_from_tray(app: tauri::AppHandle) -> Result<(), String> {
    show_main_window(&app)
}

#[tauri::command]
fn resize_launcher_window(app: tauri::AppHandle, height: f64, width: Option<f64>) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;

    let clamped_height = height.clamp(116.0, 760.0).round();
    let clamped_width = width.unwrap_or(720.0).clamp(720.0, 1280.0).round();

    // Programmatic resize usually works even when the window is not user-resizable.
    // Keep it this way to avoid focus loss caused by toggling resizable on/off.
    if window
        .set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            clamped_width,
            clamped_height,
        )))
        .is_ok()
    {
        return Ok(());
    }

    // Compatibility fallback for platforms/window managers that reject set_size on non-resizable windows.
    window
        .set_resizable(true)
        .map_err(|error| error.to_string())?;
    let resize_result = window
        .set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            clamped_width,
            clamped_height,
        )))
        .map_err(|error| error.to_string());
    let lock_result = window.set_resizable(false).map_err(|error| error.to_string());
    resize_result?;
    lock_result?;
    Ok(())
}

#[tauri::command]
fn start_window_dragging(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.start_dragging().map_err(|error| error.to_string())
}

fn plugin_entry_base_href(entry_path: &Path) -> Option<String> {
    let parent = entry_path.parent()?;
    let absolute = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
    Url::from_directory_path(absolute)
        .ok()
        .map(|url| url.to_string())
}

fn inject_plugin_base_href(html: String, base_href: Option<&str>) -> String {
    let Some(base_href) = base_href else {
        return html;
    };
    if html.contains("<base ") {
        return html;
    }

    let tag = format!(r#"<base href="{base_href}">"#);
    if html.contains("<head>") {
        return html.replacen("<head>", &format!("<head>{tag}"), 1);
    }
    if html.contains("<html>") {
        return html.replacen("<html>", &format!("<html><head>{tag}</head>"), 1);
    }
    format!("{tag}\n{html}")
}

fn hide_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.hide().map_err(|error| error.to_string())
}

fn show_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

fn toggle_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let visible = window.is_visible().map_err(|error| error.to_string())?;
    if visible {
        window.hide().map_err(|error| error.to_string())?;
        return Ok(());
    }
    show_main_window(app)
}

fn normalize_shortcut(value: &str) -> String {
    value.trim().replace(' ', "").to_lowercase()
}

fn setup_global_hotkey(app: &tauri::App, configured_hotkey: &str) {
    let normalized = normalize_shortcut(configured_hotkey);
    let use_hotkey = if normalized.is_empty() {
        "alt+space".to_string()
    } else {
        normalized
    };

    let builder = match tauri_plugin_global_shortcut::Builder::new().with_shortcuts([use_hotkey.as_str()]) {
        Ok(builder) => builder,
        Err(error) => {
            eprintln!(
                "global shortcut `{}` parse failed: {}. fallback to alt+space",
                configured_hotkey, error
            );
            match tauri_plugin_global_shortcut::Builder::new().with_shortcuts(["alt+space"]) {
                Ok(builder) => builder,
                Err(fallback_error) => {
                    eprintln!("fallback global shortcut parse failed: {}", fallback_error);
                    return;
                }
            }
        }
    };

    let plugin = builder
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let _ = toggle_main_window(app);
            }
        })
        .build();

    if let Err(error) = app.handle().plugin(plugin) {
        eprintln!("register global shortcut failed: {}", error);
    }
}

fn ensure_plugin_permission(permissions: &[String], expected: &[&str]) -> Result<(), String> {
    let lowered: Vec<String> = permissions
        .iter()
        .map(|permission| permission.trim().to_lowercase())
        .collect();
    if lowered.iter().any(|permission| permission == "*") {
        return Ok(());
    }

    let allowed = expected
        .iter()
        .any(|need| lowered.iter().any(|permission| permission == need));
    if allowed {
        return Ok(());
    }
    let expected_message = expected.join(" / ");
    Err(format!("插件缺少权限声明: {}", expected_message))
}

fn read_required_arg(args: &Value, key: &str) -> Result<String, String> {
    let Some(value) = args.get(key).and_then(|item| item.as_str()) else {
        return Err(format!("缺少参数 `{key}`"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("参数 `{key}` 不能为空"));
    }
    Ok(value.to_string())
}

fn read_required_raw_string(args: &Value, key: &str) -> Result<String, String> {
    let Some(value) = args.get(key).and_then(|item| item.as_str()) else {
        return Err(format!("缺少参数 `{key}`"));
    };
    Ok(value.to_string())
}

fn read_optional_trimmed_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|item| item.as_str())
        .map(|value| value.trim().to_string())
}

fn plugin_files_root(data_dir: &Path, plugin_id: &str) -> PathBuf {
    data_dir.join("plugin-files").join(plugin_id)
}

fn plugin_cache_file(data_dir: &Path, plugin_id: &str) -> PathBuf {
    data_dir
        .join("plugin-cache")
        .join(format!("{}.json", sanitize_path_segment(plugin_id)))
}

fn resolve_plugin_file_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let value = relative.trim();
    if value.is_empty() {
        return Err("path 不能为空".to_string());
    }

    let rel_path = Path::new(value);
    if rel_path.is_absolute() {
        return Err("仅允许插件沙盒内的相对路径".to_string());
    }

    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err("路径不合法，只允许相对路径且不能包含 ..".to_string()),
        }
    }

    Ok(root.join(rel_path))
}

fn resolve_plugin_icon_path(plugin_root: &Path, relative: &str) -> Option<PathBuf> {
    let value = relative.trim();
    if value.is_empty() {
        return None;
    }

    let rel_path = Path::new(value);
    if rel_path.is_absolute() {
        return None;
    }

    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return None,
        }
    }

    let target = plugin_root.join(rel_path);
    if target.is_file() {
        Some(target)
    } else {
        None
    }
}

fn resolve_any_path(path: &str) -> Result<PathBuf, String> {
    let value = path.trim();
    if value.is_empty() {
        return Err("path 不能为空".to_string());
    }
    Ok(PathBuf::from(value))
}

fn execute_shell_command(command: &str, cwd: Option<&str>) -> Result<Value, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("command 不能为空".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut process = {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    };

    #[cfg(not(target_os = "windows"))]
    let mut process = {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(command);
        cmd
    };

    if let Some(cwd) = cwd {
        let current = cwd.trim();
        if !current.is_empty() {
            process.current_dir(PathBuf::from(current));
        }
    }

    let output = process.output().map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(serde_json::json!({
        "success": output.status.success(),
        "code": output.status.code(),
        "stdout": stdout,
        "stderr": stderr
    }))
}

fn load_plugin_cache(file_path: &Path) -> Result<serde_json::Map<String, Value>, String> {
    if !file_path.exists() {
        return Ok(serde_json::Map::new());
    }
    let content = std::fs::read_to_string(file_path).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err("缓存文件格式异常，期望 JSON object".to_string()),
    }
}

fn save_plugin_cache(file_path: &Path, map: &serde_json::Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(map).map_err(|error| error.to_string())?;
    std::fs::write(file_path, content).map_err(|error| error.to_string())
}


fn setup_tray(app: &tauri::App) -> Result<(), String> {
    let show_item = MenuItemBuilder::with_id("show", "显示启动器")
        .build(app)
        .map_err(|error| error.to_string())?;
    let hide_item = MenuItemBuilder::with_id("hide", "隐藏到托盘")
        .build(app)
        .map_err(|error| error.to_string())?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出")
        .build(app)
        .map_err(|error| error.to_string())?;

    let menu = MenuBuilder::new(app)
        .items(&[&show_item, &hide_item, &quit_item])
        .build()
        .map_err(|error| error.to_string())?;

    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("jtools-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = show_main_window(app);
            }
            "hide" => {
                let _ = hide_main_window(app);
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    builder.build(app).map_err(|error| error.to_string())?;
    Ok(())
}

fn unpack_jtp_bytes(bytes: &[u8], output_dir: &Path) -> anyhow::Result<()> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        if enclosed.as_os_str().is_empty() {
            continue;
        }

        let target = output_dir.join(enclosed);
        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&target)?;
            continue;
        }

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&target)?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

fn detect_plugin_root(extract_root: &Path) -> anyhow::Result<PathBuf> {
    let direct_manifest = extract_root.join("manifest.json");
    if direct_manifest.exists() {
        return Ok(extract_root.to_path_buf());
    }

    let mut matched = Vec::new();
    for entry in std::fs::read_dir(extract_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        if dir.join("manifest.json").exists() {
            matched.push(dir);
        }
    }

    match matched.len() {
        1 => Ok(matched.remove(0)),
        0 => anyhow::bail!("manifest.json not found in jtp package"),
        _ => anyhow::bail!("multiple plugin roots detected in jtp package"),
    }
}

fn read_plugin_id(plugin_root: &Path) -> anyhow::Result<String> {
    let (plugin_id, _) = read_plugin_id_version(plugin_root)?;
    Ok(plugin_id)
}

fn read_plugin_id_version(plugin_root: &Path) -> anyhow::Result<(String, String)> {
    let manifest_path = plugin_root.join("manifest.json");
    let content = std::fs::read_to_string(&manifest_path)?;
    let value: serde_json::Value = serde_json::from_str(&content)?;
    let plugin_id = value
        .get("id")
        .and_then(|item| item.as_str())
        .ok_or_else(|| anyhow::anyhow!("manifest.id missing"))?;
    if plugin_id.trim().is_empty() {
        anyhow::bail!("manifest.id must not be empty");
    }
    let plugin_version = value
        .get("version")
        .and_then(|item| item.as_str())
        .unwrap_or("0.1.0");
    Ok((plugin_id.to_string(), plugin_version.to_string()))
}

fn copy_directory_blocking(source: &Path, target: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let src = entry.path();
        let dst = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory_blocking(&src, &dst)?;
        } else {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(src, dst)?;
        }
    }
    Ok(())
}

fn remove_id_from_removed_builtin(file_path: &Path, plugin_id: &str) -> anyhow::Result<()> {
    let mut removed: Vec<String> = if file_path.exists() {
        let content = std::fs::read_to_string(file_path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    let before = removed.len();
    removed.retain(|id| id != plugin_id);
    if removed.len() == before {
        return Ok(());
    }

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&removed)?;
    std::fs::write(file_path, content)?;
    Ok(())
}

fn pack_directory_as_jtp(source_dir: &Path, output_path: &Path) -> anyhow::Result<()> {
    let file = File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    add_directory_to_zip(&mut zip, source_dir, source_dir, options)?;
    zip.finish()?;
    Ok(())
}

fn add_directory_to_zip(
    zip: &mut ZipWriter<File>,
    base: &Path,
    current: &Path,
    options: FileOptions,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base)?;
        let name = rel.to_string_lossy().replace('\\', "/");
        if entry.file_type()?.is_dir() {
            let dir_name = format!("{name}/");
            zip.add_directory(dir_name, options)?;
            add_directory_to_zip(zip, base, &path, options)?;
            continue;
        }

        zip.start_file(name, options)?;
        let mut file = File::open(&path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        zip.write_all(&buffer)?;
    }
    Ok(())
}

fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

fn write_plugin_template(template_dir: &Path) -> anyhow::Result<()> {
    let files = [
        ("manifest.json", include_str!("../template/manifest.json")),
        ("README.md", include_str!("../template/README.md")),
        ("package.json", include_str!("../template/package.json")),
        ("tsconfig.json", include_str!("../template/tsconfig.json")),
        ("vite.config.js", include_str!("../template/vite.config.js")),
        (".gitignore", include_str!("../template/.gitignore")),
        ("index.html", include_str!("../template/index.html")),
        ("src/main.ts", include_str!("../template/src/main.ts")),
        (
            "src/vite-env.d.ts",
            include_str!("../template/src/vite-env.d.ts"),
        ),
        ("src/App.vue", include_str!("../template/src/App.vue")),
        (
            "src/sdk/types.ts",
            include_str!("../template/src/sdk/types.ts"),
        ),
        (
            "src/sdk/jtools.ts",
            include_str!("../template/src/sdk/jtools.ts"),
        ),
        (
            "scripts/build-jtp.mjs",
            include_str!("../template/scripts/build-jtp.mjs"),
        ),
    ];

    for (relative, content) in files {
        let target = template_dir.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, content)?;
    }
    std::fs::write(
        template_dir.join("icon.png"),
        include_bytes!("../template/icon.png"),
    )?;
    Ok(())
}

fn runtime_root(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_local_data_dir()
        .unwrap_or_else(|_| PathBuf::from("../runtime"))
        .join("runtime")
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let root_dir = runtime_root(&app.handle());
            let platform = tauri::async_runtime::block_on(DesktopPlatform::bootstrap(root_dir))
                .map_err(|error| error.to_string())?;
            let startup_hotkey = platform.settings.hotkey.clone();

            app.manage(AppState {
                platform: Mutex::new(platform),
            });
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_shadow(false);
            }
            setup_tray(app)?;
            setup_global_hotkey(app, &startup_hotkey);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let WindowEvent::CloseRequested { api, .. } = event {
                let should_close_to_tray = {
                    let state = window.state::<AppState>();
                    tauri::async_runtime::block_on(async {
                        let platform = state.platform.lock().await;
                        platform.settings.window_behavior.close_to_tray
                    })
                };
                if should_close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            get_user_preferences,
            update_user_preferences,
            list_manage_commands,
            search,
            list_plugins,
            set_plugin_enabled,
            uninstall_plugin,
            install_plugin_from_jtp,
            install_plugin_from_jtp_dialog,
            export_plugin_to_jtp,
            pack_jtp_from_directory,
            pack_jtp_from_dialog,
            download_plugin_template,
            download_plugin_template_dialog,
            search_in_plugin,
            plugin_display_name,
            plugin_view_options,
            plugin_window_icon_bytes,
            plugin_view_html,
            capability_open_url,
            capability_copy_text,
            plugin_capability_call,
            execute_item,
            reload_plugins,
            hide_launcher_to_tray,
            show_launcher_from_tray,
            resize_launcher_window,
            start_window_dragging
        ])
        .run(tauri::generate_context!())
        .expect("failed to run jtools desktop");
}
