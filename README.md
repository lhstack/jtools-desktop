# jtools

`jtools` 是一个基于 **Rust Core + Tauri Desktop + 前端启动器 UI** 的插件化桌面启动器。

## 版本说明

- 当前版本：`0.0.2`
- 版本状态：稳定性迭代版本，重点在启动器输入体验、窗口行为与跨平台构建流程完善。

核心能力：

- 全局热键唤起（默认 `Alt+Space`）
- 搜索命令/插件动作并执行
- 插件管理中心（启用/禁用、卸载、导入 `.jtp`、模板下载）
- 插件能力网关（剪贴板、URL、文件、缓存、命令执行等）
- 托盘驻留、`Esc` 隐藏、窗口动态高度

---

## 技术栈

- Core: Rust
- Desktop Shell: Tauri v2
- Frontend: Vite + TypeScript（仓库主前端）
- 插件模板: Vite + Vue3 + TypeScript（位于 `src-tauri/template`）

---

## 目录说明

```text
.
├─ src/                  # Rust core（搜索、插件、状态、能力）
├─ src-web/              # Tauri 前端页面逻辑（启动器/管理中心）
├─ src-tauri/            # Tauri 入口、命令、打包配置
├─ plugins/builtin/      # 内置插件
├─ runtime/              # 本地运行时数据（已 gitignore）
└─ src-tauri/template/   # 插件开发模板（Vue3+TS）
```

---

## 本地开发

安装依赖：

```bash
bun install
```

仅运行 Rust CLI（调试 core）：

```bash
cargo run
cargo run -- search 设置
cargo run -- exec 设置
```

运行桌面应用（Tauri Dev）：

```bash
bun run tauri dev
```

---

## 构建与发布

前端构建：

```bash
bun run build
```

桌面打包（Windows，NSIS）：

```bash
bun run tauri:build:win
```

安装包输出目录：

```text
src-tauri/target/release/bundle/nsis/
```

---

## Windows 打包注意事项

1. 发布版已关闭控制台黑窗（`windows_subsystem = "windows"`）。
2. `tauri.conf.json` 为跨平台默认配置；Windows 专用打包配置在 `src-tauri/tauri.windows.conf.json`。
3. Windows 打包请使用 `bun run tauri:build:win`，避免在 macOS/Linux 上加载 Windows 专用配置。
4. 若目标机器缺少 WebView2 Runtime，请按系统提示安装（或由安装器引导安装）。

---

## macOS 安装提示（“已损坏”）

从未签名或未公证的构建产物安装时，macOS 可能提示“App 已损坏”或“无法验证开发者”。

可按下面步骤处理：

1. 在 Finder 中对 `jtools.app` 右键，选择“打开”，再二次确认打开。
2. 若仍被拦截，在终端执行（仅对可信来源包执行）：

```bash
sudo xattr -dr com.apple.quarantine /Applications/jtools.app
```

如果你的 App 不在 `/Applications`，请替换为实际路径。

---

## 插件系统（jtp）

管理中心支持：

- 导入 `.jtp`
- 目录打包 `.jtp`
- 下载插件开发模板

`.jtp` 包结构要求（根目录）：

```text
manifest.json
index.html
assets/...
```

---

## 插件模板（Vue3 + TS）

模板位置：

```text
src-tauri/template
```

模板特点：

- `src/sdk/jtools.ts`：TS SDK 工具包封装
- `scripts/build-jtp.mjs`：自动打包 `.jtp`
- `vite-plugin-singlefile`：构建为单文件 `index.html`，规避 `srcdoc` 场景下资源拦截

模板详细说明见：

[`src-tauri/template/README.md`](./src-tauri/template/README.md)

---

## 常用命令速查

```bash
# 前端开发
bun run dev

# Tauri 开发模式
bun run tauri dev

# 前端构建
bun run build

# 打包安装程序（Windows NSIS）
bun run tauri:build:win
```
