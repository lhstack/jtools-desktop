<template>
  <main class="page">
    <section class="card">
      <h1 class="title">jtools Vue3 + TS 插件模板</h1>
      <p class="hint">{{ hintText }}</p>

      <section class="group">
        <h2>系统能力</h2>
        <div class="row">
          <button class="btn" type="button" @click="onOpenUrl">open_url</button>
          <button class="btn" type="button" @click="onCopyText">copy_text</button>
          <button class="btn" type="button" @click="onReadClipboard">read_clipboard</button>
          <button class="btn" type="button" @click="onHideToTray">hide_to_tray</button>
          <button class="btn" type="button" @click="onShowLauncher">show_launcher</button>
          <button class="btn" type="button" @click="onReloadPlugins">reload_plugins</button>
        </div>
      </section>

      <section class="group">
        <h2>沙盒文件（runtime/data/plugin-files/&lt;plugin_id&gt;）</h2>
        <div class="row">
          <button class="btn" type="button" @click="onSandboxWrite">file_write_text</button>
          <button class="btn" type="button" @click="onSandboxAppend">file_append_text</button>
          <button class="btn" type="button" @click="onSandboxRead">file_read_text</button>
          <button class="btn" type="button" @click="onSandboxExists">file_exists</button>
          <button class="btn" type="button" @click="onSandboxList">file_list_dir</button>
          <button class="btn" type="button" @click="onSandboxRemove">file_remove</button>
        </div>
      </section>

      <section class="group">
        <h2>系统路径文件（插件自行处理越权）</h2>
        <div class="row">
          <input v-model="state.systemPath" class="input" />
        </div>
        <div class="row with-margin">
          <button class="btn" type="button" @click="onPathWrite">file_write_path</button>
          <button class="btn" type="button" @click="onPathAppend">file_append_path</button>
          <button class="btn" type="button" @click="onPathRead">file_read_path</button>
          <button class="btn" type="button" @click="onPathRemove">file_remove_path</button>
        </div>
      </section>

      <section class="group">
        <h2>命令执行（cmd/sh）</h2>
        <div class="row">
          <input v-model="state.shellCommand" class="input" />
        </div>
        <div class="row with-margin">
          <button class="btn" type="button" @click="onExecShell">exec_shell</button>
        </div>
      </section>

      <section class="group">
        <h2>缓存（runtime/data/plugin-cache/&lt;plugin_id&gt;.json）</h2>
        <div class="row">
          <button class="btn" type="button" @click="onCacheSet">cache_set</button>
          <button class="btn" type="button" @click="onCacheGet">cache_get</button>
          <button class="btn" type="button" @click="onCacheList">cache_list_keys</button>
          <button class="btn" type="button" @click="onCacheDelete">cache_delete</button>
          <button class="btn" type="button" @click="onCacheClear">cache_clear</button>
        </div>
      </section>

      <section class="list" v-if="state.items.length > 0">
        <button
          v-for="item in state.items"
          :key="item.title + (item.subtitle ?? '')"
          class="item"
          type="button"
          @click="executeItem(item)"
          @dblclick="executeItem(item)"
        >
          <span class="item-title">{{ item.title }}</span>
          <span class="item-subtitle">{{ item.subtitle }}</span>
        </button>
      </section>

      <pre class="log">{{ logsText }}</pre>
    </section>
  </main>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive } from "vue";
import { jtools } from "./sdk/jtools";
import type { HostMessageData, HookResultItem, PluginCandidateItem } from "./sdk/types";

const PLUGIN_ID = "demo-plugin-vue";
const SANDBOX_FILE = "notes/demo.log";

const state = reactive({
  query: "",
  items: [] as PluginCandidateItem[],
  logs: [] as string[],
  systemPath: "C:\\temp\\jtools-system-demo-vue.txt",
  shellCommand: "echo hello-from-vue-plugin",
});

function pushLog(title: string, payload: unknown): void {
  const body = typeof payload === "string" ? payload : JSON.stringify(payload, null, 2);
  state.logs.unshift(`[${new Date().toLocaleTimeString()}] ${title}\n${body}`);
}

async function runAndLog(title: string, runner: () => Promise<unknown>): Promise<void> {
  try {
    const result = await runner();
    pushLog(title, result);
  } catch (error) {
    pushLog(`${title} (失败)`, String(error));
  }
}

function buildItems(query: string): PluginCandidateItem[] {
  const value = query.trim();
  if (!value) return [];
  return [
    {
      title: `搜索: ${value}`,
      subtitle: "打开网页示例",
      action: {
        capability: "open_url",
        args: { url: `https://www.baidu.com/s?wd=${encodeURIComponent(value)}` },
      },
    },
    {
      title: `复制: ${value}`,
      subtitle: "复制文本示例",
      action: {
        capability: "copy_text",
        args: { text: value },
      },
    },
  ];
}

function toHookItem(item: PluginCandidateItem, index: number): HookResultItem {
  const action =
    item.action.capability === "open_url"
      ? { open_url: item.action.args?.url }
      : { copy_text: item.action.args?.text };
  return {
    title: item.title,
    subtitle: item.subtitle,
    score: 86 - index * 2,
    action,
  };
}

function resizeByItemsCount(): void {
  jtools.setHeight(state.items.length > 0 ? 640 : 520);
}

async function executeItem(item?: PluginCandidateItem): Promise<void> {
  if (!item?.action?.capability) return;
  await runAndLog(`执行: ${item.action.capability}`, () =>
    jtools.call(item.action.capability, item.action.args ?? {}),
  );
}

function onHostMessage(event: MessageEvent<HostMessageData>): void {
  const data = event.data;
  if (!data || typeof data !== "object" || !("type" in data)) return;

  if (data.type === "jtools-host-query") {
    state.query = String(data.query ?? "");
    state.items = buildItems(state.query);
    resizeByItemsCount();
    return;
  }

  if (data.type === "jtools-host-submit") {
    void executeItem(state.items[0]);
    return;
  }

  if (data.type === "jtools-hook-search") {
    const query = String(data.query ?? "");
    const items = buildItems(query).map(toHookItem);
    jtools.postHookResults(Number(data.token ?? 0), items);
  }
}

onMounted(() => {
  jtools.configure({ pluginId: PLUGIN_ID });
  window.addEventListener("message", onHostMessage);
  window.parent.postMessage({ type: "jtools-plugin-ready" }, "*");
  resizeByItemsCount();
  pushLog("初始化", "模板已就绪（Vue3 + TS + src/sdk）。");
});

onBeforeUnmount(() => {
  window.removeEventListener("message", onHostMessage);
});

const hintText = computed(() =>
  state.items.length > 0
    ? `当前 ${state.items.length} 项候选，可点击或双击执行。`
    : "输入关键词后会展示候选，回车执行第一项。",
);
const logsText = computed(() => state.logs.join("\n\n"));

function onOpenUrl(): Promise<void> {
  return runAndLog("open_url", () => jtools.capabilities.openUrl("https://www.baidu.com"));
}
function onCopyText(): Promise<void> {
  return runAndLog("copy_text", () =>
    jtools.capabilities.copyText(`copied at ${new Date().toISOString()}`),
  );
}
function onReadClipboard(): Promise<void> {
  return runAndLog("read_clipboard", () => jtools.capabilities.readClipboard());
}
function onHideToTray(): Promise<void> {
  return runAndLog("hide_to_tray", () => jtools.capabilities.hideToTray());
}
function onShowLauncher(): Promise<void> {
  return runAndLog("show_launcher", () => jtools.capabilities.showLauncher());
}
function onReloadPlugins(): Promise<void> {
  return runAndLog("reload_plugins", () => jtools.capabilities.reloadPlugins());
}

function onSandboxWrite(): Promise<void> {
  return runAndLog("file_write_text", async () => {
    await jtools.capabilities.fileCreateDir("notes");
    return jtools.capabilities.fileWriteText(
      SANDBOX_FILE,
      `first line at ${new Date().toISOString()}\n`,
    );
  });
}
function onSandboxAppend(): Promise<void> {
  return runAndLog("file_append_text", () =>
    jtools.capabilities.fileAppendText(
      SANDBOX_FILE,
      `append line: query=${state.query || "(empty)"}\n`,
    ),
  );
}
function onSandboxRead(): Promise<void> {
  return runAndLog("file_read_text", () => jtools.capabilities.fileReadText(SANDBOX_FILE));
}
function onSandboxExists(): Promise<void> {
  return runAndLog("file_exists", () => jtools.capabilities.fileExists(SANDBOX_FILE));
}
function onSandboxList(): Promise<void> {
  return runAndLog("file_list_dir", () => jtools.capabilities.fileListDir("notes"));
}
function onSandboxRemove(): Promise<void> {
  return runAndLog("file_remove", () => jtools.capabilities.fileRemove(SANDBOX_FILE));
}

function onPathWrite(): Promise<void> {
  return runAndLog("file_write_path", () =>
    jtools.capabilities.fileWritePath(state.systemPath, `write at ${new Date().toISOString()}\n`),
  );
}
function onPathAppend(): Promise<void> {
  return runAndLog("file_append_path", () =>
    jtools.capabilities.fileAppendPath(
      state.systemPath,
      `append at ${new Date().toISOString()}\n`,
    ),
  );
}
function onPathRead(): Promise<void> {
  return runAndLog("file_read_path", () => jtools.capabilities.fileReadPath(state.systemPath));
}
function onPathRemove(): Promise<void> {
  return runAndLog("file_remove_path", () => jtools.capabilities.fileRemovePath(state.systemPath));
}

function onExecShell(): Promise<void> {
  return runAndLog("exec_shell", () => jtools.capabilities.execShell(state.shellCommand));
}

function onCacheSet(): Promise<void> {
  return runAndLog("cache_set", () =>
    jtools.capabilities.cacheSet("demo.last", { query: state.query, at: Date.now() }),
  );
}
function onCacheGet(): Promise<void> {
  return runAndLog("cache_get", () => jtools.capabilities.cacheGet("demo.last"));
}
function onCacheList(): Promise<void> {
  return runAndLog("cache_list_keys", () => jtools.capabilities.cacheListKeys());
}
function onCacheDelete(): Promise<void> {
  return runAndLog("cache_delete", () => jtools.capabilities.cacheDelete("demo.last"));
}
function onCacheClear(): Promise<void> {
  return runAndLog("cache_clear", () => jtools.capabilities.cacheClear());
}
</script>

<style scoped>
.page {
  margin: 0;
  padding: 12px;
  background: #f7f8fb;
  color: #1f2433;
  font-family: "Segoe UI Variable Text", "Microsoft YaHei UI", sans-serif;
}
.card {
  border: 1px solid #d9dfea;
  border-radius: 12px;
  background: #ffffff;
  padding: 12px;
}
.title {
  margin: 0;
  font-size: 18px;
  font-weight: 700;
}
.hint {
  margin: 6px 0 0;
  color: #5d6a83;
  font-size: 13px;
}
.group {
  margin-top: 10px;
  border: 1px solid #e1e7f8;
  border-radius: 10px;
  padding: 8px;
  background: #fafcff;
}
.group h2 {
  margin: 0 0 6px;
  font-size: 13px;
}
.row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.with-margin {
  margin-top: 8px;
}
.input {
  width: 100%;
  min-width: 220px;
  border: 1px solid #cad5f2;
  border-radius: 8px;
  padding: 6px 8px;
  font-size: 12px;
}
.btn {
  border: 1px solid #ced7f6;
  border-radius: 8px;
  background: #edf1ff;
  color: #2d4bb8;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 10px;
  cursor: pointer;
}
.btn:hover {
  background: #e3ebff;
}
.list {
  margin-top: 10px;
  display: grid;
  gap: 8px;
}
.item {
  border: 1px solid #d6def4;
  border-radius: 9px;
  padding: 8px;
  background: #fdfdff;
  cursor: pointer;
  text-align: left;
}
.item:hover {
  border-color: #9eb2f9;
  background: #f3f6ff;
}
.item-title {
  display: block;
  font-size: 14px;
  font-weight: 600;
}
.item-subtitle {
  display: block;
  margin-top: 2px;
  color: #596682;
  font-size: 12px;
}
.log {
  margin-top: 10px;
  border: 1px solid #d7def2;
  border-radius: 8px;
  background: #f5f8ff;
  padding: 8px;
  font-size: 12px;
  color: #45506a;
  white-space: pre-wrap;
  max-height: 240px;
  overflow: auto;
}
</style>
