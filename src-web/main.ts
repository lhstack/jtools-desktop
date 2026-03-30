import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createApp, h, type App as VueApp } from "vue";
import ElementPlus, { ElAside, ElButton, ElContainer, ElMain, ElMenu, ElMenuItem } from "element-plus";
import "element-plus/dist/index.css";
import "./style.css";

type SearchAction =
  | { host_command: string }
  | { plugin_command: { plugin_id: string; command_id: string; mode: string } }
  | { open_url: string }
  | { copy_text: string };

type SearchItem = {
  source_type: string;
  source_id: string;
  plugin_id: string | null;
  title: string;
  subtitle: string;
  keywords: string[];
  score: number;
  action: SearchAction;
};

type AppStatus = {
  hotkey: string;
  rootDir: string;
  totalPlugins: number;
  enabledPlugins: number;
  disabledPlugins: number;
  faultedPlugins: number;
};

type PluginListItem = {
  id: string;
  name: string;
  description: string;
  commandCount: number;
  enabled: boolean;
};

type UserPreferences = {
  theme: string;
  language: string;
  hotkey: string;
  maxResults: number;
  includeRecent: boolean;
  hideOnBlur: boolean;
  closeToTray: boolean;
  rootDir: string;
  pluginsDir: string;
};

type ManageCommandItem = {
  id: string;
  title: string;
  subtitle: string;
  pluginId: string | null;
  commandId: string | null;
  mode: string | null;
  enabled: boolean;
  keywords: string[];
  action: SearchAction;
};

type PluginViewOptions = {
  showSearchInput: boolean;
};

type ActivePlugin = {
  id: string;
  name: string;
  showSearchInput: boolean;
};

type ManageSection = "plugins" | "shortcuts" | "commands" | "settings";

type HookCandidate = {
  title: string;
  subtitle?: string;
  score?: number;
  action: SearchAction;
};

type PluginMessage =
  | { type: "jtools-plugin-ready" }
  | { type: "jtools-plugin-height"; height: number }
  | { type: "jtools-plugin-action"; action: "open_url"; url: string }
  | { type: "jtools-plugin-action"; action: "copy_text"; text: string }
  | { type: "jtools-plugin-capability"; requestId?: string; capability: string; args?: Record<string, unknown> }
  | { type: "jtools-hook-results"; pluginId: string; token: number; items: HookCandidate[] };

type PluginCapabilityResponse = {
  requestId?: string;
  ok: boolean;
  message: string;
  data?: Record<string, unknown> | null;
};

type HookFrame = {
  pluginId: string;
  pluginName: string;
  frame: HTMLIFrameElement;
  ready: boolean;
};

type Elements = {
  shell: HTMLElement;
  launcherBody: HTMLElement;
  manageCenter: HTMLElement;
  manageOpenButton: HTMLButtonElement;
  manageCloseButton: HTMLButtonElement;
  manageContentBody: HTMLDivElement;
  hookHost: HTMLElement;
  searchBox: HTMLElement;
  input: HTMLInputElement;
  reloadButton: HTMLButtonElement;
  panel: HTMLElement;
  results: HTMLDivElement;
  count: HTMLSpanElement;
  empty: HTMLParagraphElement;
  hotkey: HTMLSpanElement;
  pluginContext: HTMLElement;
  pluginName: HTMLSpanElement;
  pluginOpenWindow: HTMLButtonElement;
  pluginClear: HTMLButtonElement;
  pluginHost: HTMLElement;
  pluginFrame: HTMLIFrameElement;
  pluginHostEmpty: HTMLParagraphElement;
  toast: HTMLDivElement;
};

class SearchApp {
  private readonly launcherWidth = 1120;
  private readonly managerWidth = 1120;
  private readonly managerHeight = 756;
  private readonly maxWindowHeight = 760;
  private readonly minWindowHeight = 116;
  private readonly pluginViewportMinHeight = 120;
  private readonly pluginViewportMaxHeight = 2000;

  private app = document.querySelector<HTMLDivElement>("#app");
  private elements: Elements | null = null;
  private status: AppStatus | null = null;
  private plugins: PluginListItem[] = [];
  private enabledPluginIds = new Set<string>();
  private manageMode = false;
  private manageSection: ManageSection = "plugins";
  private manageNotice = "";
  private preferences: UserPreferences | null = null;
  private manageCommands: ManageCommandItem[] = [];
  private manageCommandFilter = "";
  private query = "";
  private hasSearched = false;
  private results: SearchItem[] = [];
  private selectedIndex = 0;
  private searchTimer: number | null = null;
  private searchToken = 0;
  private lastAppliedHeight = 0;
  private lastAppliedWidth = 0;
  private focusRecoveryTimers: number[] = [];
  private isInputComposing = false;
  private activePlugin: ActivePlugin | null = null;
  private pendingPluginQuery = "";
  private pluginFrameReady = false;
  private hookToken = 0;
  private hookFrames = new Map<string, HookFrame>();
  private hookExpected = new Map<number, Set<string>>();
  private hookBuckets = new Map<number, SearchItem[]>();
  private hookResolvers = new Map<number, (items: SearchItem[]) => void>();
  private manageVueApp: VueApp | null = null;
  private toastTimer: number | null = null;

  async init() {
    if (!this.app) {
      throw new Error("App root not found");
    }

    this.mount();
    await listen("jtools://launcher-shown", () => {
      this.scheduleFocusRecovery();
    });
    await this.loadStatus();
    await this.prepareHookFrames();
    await this.render();
    await this.resizeWindowToContent();
  }

  private mount() {
    if (!this.app) {
      return;
    }

    this.app.innerHTML = `
      <main class="launcher">
        <section class="launcher-shell" id="launcher-shell" data-tauri-drag-region>
          <header class="chrome">
            <div class="brand">
              <span class="brand-mark">J</span>
              <span class="brand-name">jtools</span>
            </div>
            <div class="chrome-meta">
              <span class="hotkey-chip" id="hotkey-label">Alt+Space</span>
              <button class="icon-button" id="reload-button" type="button" title="重载插件">↻</button>
              <button class="manager-button" id="manage-open-button" type="button" title="管理中心">⚙</button>
            </div>
          </header>

          <section id="launcher-body">
            <section class="plugin-context hidden" id="plugin-context">
              <div class="plugin-chip">
                <span id="plugin-name"></span>
                <button id="plugin-open-window" class="plugin-action-button" type="button" title="在新窗口打开">新窗口</button>
                <button id="plugin-clear" class="plugin-clear" type="button" title="关闭插件">×</button>
              </div>
            </section>

            <section class="search-box" id="search-box">
              <span class="search-icon">⌕</span>
              <input
                id="search-input"
                class="search-input"
                type="text"
                autocomplete="off"
                autocapitalize="off"
                autocorrect="off"
                spellcheck="false"
                placeholder="搜索插件、命令和动作"
              />
            </section>

            <section class="plugin-host hidden" id="plugin-host">
              <iframe
                id="plugin-frame"
                class="plugin-frame"
                sandbox="allow-scripts allow-popups allow-forms allow-modals"
              ></iframe>
              <p class="plugin-host-empty hidden" id="plugin-host-empty">插件页面不可用</p>
            </section>

            <section class="results-panel hidden" id="results-panel">
              <div class="status-row">
                <span>搜索结果</span>
                <span id="result-count">0 项</span>
              </div>
              <div class="results" id="results"></div>
              <p class="empty" id="empty-state">没有匹配结果。</p>
            </section>
          </section>

          <section class="manage-center hidden" id="manage-center">
            <div id="manage-vue-root"></div>
          </section>

          <section class="hook-host hidden" id="hook-host"></section>
          <div class="global-toast hidden" id="global-toast"></div>
        </section>
      </main>
    `;

    const shell = this.queryElement<HTMLElement>("#launcher-shell");
    const launcherBody = this.queryElement<HTMLElement>("#launcher-body");
    const manageCenter = this.queryElement<HTMLElement>("#manage-center");
    const manageVueRoot = this.queryElement<HTMLElement>("#manage-vue-root");
    this.mountManageShell(manageVueRoot);
    const manageOpenButton = this.queryElement<HTMLButtonElement>("#manage-open-button");
    const manageCloseButton = this.queryElement<HTMLButtonElement>("#manage-close-button");
    const manageContentBody = this.queryElement<HTMLDivElement>("#manage-content-body");
    const hookHost = this.queryElement<HTMLElement>("#hook-host");
    const searchBox = this.queryElement<HTMLElement>("#search-box");
    const input = this.queryElement<HTMLInputElement>("#search-input");
    const reloadButton = this.queryElement<HTMLButtonElement>("#reload-button");
    const panel = this.queryElement<HTMLElement>("#results-panel");
    const results = this.queryElement<HTMLDivElement>("#results");
    const count = this.queryElement<HTMLSpanElement>("#result-count");
    const empty = this.queryElement<HTMLParagraphElement>("#empty-state");
    const hotkey = this.queryElement<HTMLSpanElement>("#hotkey-label");
    const pluginContext = this.queryElement<HTMLElement>("#plugin-context");
    const pluginName = this.queryElement<HTMLSpanElement>("#plugin-name");
    const pluginOpenWindow = this.queryElement<HTMLButtonElement>("#plugin-open-window");
    const pluginClear = this.queryElement<HTMLButtonElement>("#plugin-clear");
    const pluginHost = this.queryElement<HTMLElement>("#plugin-host");
    const pluginFrame = this.queryElement<HTMLIFrameElement>("#plugin-frame");
    const pluginHostEmpty = this.queryElement<HTMLParagraphElement>("#plugin-host-empty");
    const toast = this.queryElement<HTMLDivElement>("#global-toast");

    this.elements = {
      shell,
      launcherBody,
      manageCenter,
      manageOpenButton,
      manageCloseButton,
      manageContentBody,
      hookHost,
      searchBox,
      input,
      reloadButton,
      panel,
      results,
      count,
      empty,
      hotkey,
      pluginContext,
      pluginName,
      pluginOpenWindow,
      pluginClear,
      pluginHost,
      pluginFrame,
      pluginHostEmpty,
      toast,
    };

    shell.addEventListener("mousedown", (event) => {
      const target = event.target as HTMLElement | null;
      if (event.button !== 0) {
        return;
      }
      if (
        target?.closest(
          "input, button, .results, .result-card, .plugin-context, .plugin-host, .manage-center, .manage-layout",
        )
      ) {
        return;
      }
      void invoke("start_window_dragging");
    });

    input.addEventListener("input", (event) => {
      const value = (event.target as HTMLInputElement).value;
      this.scheduleSearch(value);
    });
    input.addEventListener("compositionstart", () => {
      this.isInputComposing = true;
    });
    input.addEventListener("compositionend", () => {
      this.isInputComposing = false;
    });

    input.addEventListener("keydown", async (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (this.shouldEscapeHideToTray()) {
          await this.hideToTray();
          return;
        }
        if (this.activePlugin) {
          await this.clearActivePlugin();
          return;
        }
        this.query = "";
        this.hasSearched = false;
        this.results = [];
        this.selectedIndex = 0;
        input.value = "";
        await this.render();
        return;
      }

      if (this.activePlugin) {
        if (event.key === "Enter") {
          event.preventDefault();
          this.submitPluginQuery();
        }
        return;
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        this.moveSelection(1);
        await this.render();
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        this.moveSelection(-1);
        await this.render();
      } else if (event.key === "Enter") {
        event.preventDefault();
        const item = this.results[this.selectedIndex];
        if (!item) {
          return;
        }

        if (this.isPluginCommand(item)) {
          await this.activatePlugin(item);
          return;
        }

        await this.execute(item);
      }
    });

    manageOpenButton.addEventListener("click", async () => {
      await this.openManageCenter();
    });

    manageCloseButton.addEventListener("click", async () => {
      await this.closeManageCenter();
    });

    shell.querySelectorAll<HTMLElement>(".manage-side-item[data-section]").forEach((item) => {
      item.addEventListener("click", async () => {
        const section = item.dataset.section as ManageSection | undefined;
        if (!section) {
          return;
        }
        this.manageSection = section;
        await this.render();
      });
    });

    pluginClear.addEventListener("click", async () => {
      await this.clearActivePlugin();
    });
    pluginOpenWindow.addEventListener("click", async () => {
      await this.openActivePluginInNewWindow();
    });

    reloadButton.addEventListener("click", async () => {
      await invoke<string>("reload_plugins");
      await this.loadStatus();
      await this.prepareHookFrames();
      if (!this.activePlugin && this.query.trim()) {
        await this.performSearch(this.query.trim());
      } else if (this.activePlugin) {
        await this.loadPluginView();
      }
      await this.render();
    });

    pluginFrame.addEventListener("load", () => {
      this.pluginFrameReady = true;
      this.pushQueryToPlugin(this.pendingPluginQuery);
    });

    window.addEventListener("message", this.onPluginMessage);
    window.addEventListener("keydown", this.onWindowKeydown);
    window.addEventListener("focus", this.onHostWindowActivated);
    document.addEventListener("visibilitychange", this.onVisibilityChange);
    this.scheduleFocusRecovery();
  }

  private mountManageShell(root: HTMLElement) {
    if (this.manageVueApp) {
      this.manageVueApp.unmount();
      this.manageVueApp = null;
    }

    const sideItem = (label: string, section: ManageSection, active = false) =>
      h(
        ElMenuItem,
        {
          class: ["manage-side-item", active ? "is-active" : ""],
          index: section,
          "data-section": section,
        },
        () => label,
      );

    const ManageShell = {
      render: () =>
        h(ElContainer, { class: "manage-shell", direction: "vertical" }, () => [
          h("header", { class: "manage-topbar" }, [
            h("div", { class: "manage-tab" }, "管理中心"),
            h(
              ElButton,
              {
                id: "manage-close-button",
                class: "manage-close",
                size: "small",
                plain: true,
              },
              () => "返回启动器",
            ),
          ]),
          h(
            ElContainer,
            { class: "manage-layout" },
            () => [
              h(
                ElAside,
                { class: "manage-sidebar", width: "220px" },
                () => [
                  h("div", { class: "manage-sidebar-body" }, [
                    h("h3", { class: "manage-side-title" }, "偏好设置"),
                    h(
                      ElMenu,
                      {
                        class: "manage-menu",
                        defaultActive: "plugins",
                        collapseTransition: false,
                      },
                      () => [
                        sideItem("插件管理", "plugins", true),
                        sideItem("快捷方式", "shortcuts"),
                        sideItem("所有命令", "commands"),
                        sideItem("设置", "settings"),
                      ],
                    ),
                  ]),
                ],
              ),
              h(ElMain, { class: "manage-content" }, () =>
                h("div", { id: "manage-content-body", class: "manage-content-body" }),
              ),
            ],
          ),
        ]),
    };

    const app = createApp(ManageShell);
    app.use(ElementPlus);
    app.mount(root);
    this.manageVueApp = app;
  }

  private onWindowKeydown = async (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      if (this.manageMode && !this.shouldEscapeHideToTray()) {
        await this.closeManageCenter();
        return;
      }
      if (this.shouldEscapeHideToTray()) {
        await this.hideToTray();
      }
    }
  };

  private onHostWindowActivated = () => {
    this.scheduleFocusRecovery();
  };

  private onVisibilityChange = () => {
    if (!document.hidden) {
      this.scheduleFocusRecovery();
    }
  };

  private onPluginMessage = async (event: MessageEvent<unknown>) => {
    if (!this.elements) {
      return;
    }

    const data = event.data as PluginMessage | null;
    if (!data || typeof data !== "object" || !("type" in data)) {
      return;
    }

    if (data.type === "jtools-hook-results") {
      const expected = this.hookExpected.get(data.token);
      if (!expected || !expected.has(data.pluginId)) {
        return;
      }

      const bucket = this.hookBuckets.get(data.token) ?? [];
      const pluginName = this.hookFrames.get(data.pluginId)?.pluginName ?? data.pluginId;
      const mapped = data.items.map((item, index) => {
        const score = Number.isFinite(item.score) ? Number(item.score) : 79;
        return {
          source_type: "plugin_hook",
          source_id: `${data.pluginId}:${index}:${item.title}`,
          plugin_id: data.pluginId,
          title: item.title,
          subtitle: item.subtitle || pluginName,
          keywords: [],
          score,
          action: item.action,
        } as SearchItem;
      });
      bucket.push(...mapped);
      this.hookBuckets.set(data.token, bucket);
      expected.delete(data.pluginId);
      if (expected.size === 0) {
        const resolve = this.hookResolvers.get(data.token);
        if (resolve) {
          resolve(bucket);
          this.hookResolvers.delete(data.token);
        }
      }
      return;
    }

    if (data.type === "jtools-plugin-capability") {
      const pluginId = this.resolvePluginIdBySource(event.source);
      const requestId = typeof data.requestId === "string" ? data.requestId : "";
      if (!pluginId) {
        this.postToMessageSource(event.source, {
          type: "jtools-host-capability-result",
          requestId,
          ok: false,
          message: "未识别插件来源",
        });
        return;
      }

      try {
        const result = await invoke<PluginCapabilityResponse>("plugin_capability_call", {
          payload: {
            pluginId,
            capability: data.capability,
            requestId,
            args: data.args ?? {},
          },
        });
        this.postToMessageSource(event.source, {
          type: "jtools-host-capability-result",
          requestId: result.requestId ?? requestId,
          ok: result.ok,
          message: result.message,
          data: result.data ?? null,
        });
      } catch (error) {
        this.postToMessageSource(event.source, {
          type: "jtools-host-capability-result",
          requestId,
          ok: false,
          message: String(error),
        });
      }
      return;
    }

    if (!this.activePlugin) {
      return;
    }
    if (event.source !== this.elements.pluginFrame.contentWindow) {
      return;
    }

    if (data.type === "jtools-plugin-ready") {
      this.pluginFrameReady = true;
      this.pushQueryToPlugin(this.pendingPluginQuery);
      return;
    }

    if (data.type === "jtools-plugin-height") {
      if (Number.isFinite(data.height)) {
        const clamped = Math.max(
          this.pluginViewportMinHeight,
          Math.min(this.pluginViewportMaxHeight, Math.round(data.height)),
        );
        if (this.elements) {
          this.elements.pluginFrame.style.height = `${clamped}px`;
        }
        await this.resizeWindowToContent();
      }
      return;
    }

    if (data.type === "jtools-plugin-action") {
      try {
        if (data.action === "open_url") {
          await invoke("capability_open_url", { url: data.url });
        } else if (data.action === "copy_text") {
          await invoke("capability_copy_text", { text: data.text });
          this.showToast("已复制到剪贴板");
        }
      } catch (error) {
        console.error("plugin action failed", error);
      }
    }
  };

  private postToMessageSource(source: MessageEventSource | null, payload: Record<string, unknown>) {
    if (!source || typeof source !== "object" || !("postMessage" in source)) {
      return;
    }
    const target = source as WindowProxy;
    target.postMessage(payload, "*");
  }

  private resolvePluginIdBySource(source: MessageEventSource | null): string | null {
    if (!source || !this.elements) {
      return null;
    }

    if (this.activePlugin && source === this.elements.pluginFrame.contentWindow) {
      return this.activePlugin.id;
    }

    for (const hook of this.hookFrames.values()) {
      if (source === hook.frame.contentWindow) {
        return hook.pluginId;
      }
    }
    return null;
  }

  private queryElement<T extends Element>(selector: string): T {
    const element = this.app?.querySelector<T>(selector);
    if (!element) {
      throw new Error(`Required element not found: ${selector}`);
    }
    return element;
  }

  private async loadStatus() {
    const status = await invoke<AppStatus>("get_app_status");
    this.status = status;
    if (this.elements) {
      this.elements.hotkey.textContent = status.hotkey;
    }
  }

  private async refreshManageData() {
    await this.loadStatus();
    try {
      const [plugins, preferences, commands] = await Promise.all([
        invoke<PluginListItem[]>("list_plugins"),
        invoke<UserPreferences>("get_user_preferences"),
        invoke<ManageCommandItem[]>("list_manage_commands"),
      ]);
      this.plugins = plugins;
      this.preferences = preferences;
      this.manageCommands = commands;
      this.enabledPluginIds = new Set(
        this.plugins.filter((plugin) => plugin.enabled).map((plugin) => plugin.id),
      );
      await this.prepareHookFrames(this.plugins);
    } catch (error) {
      console.error("list_plugins failed", error);
      this.plugins = [];
      this.preferences = null;
      this.manageCommands = [];
      this.enabledPluginIds.clear();
    }
  }

  private async prepareHookFrames(existingList?: PluginListItem[]) {
    const list =
      existingList ??
      (() => {
        if (this.plugins.length > 0) {
          return this.plugins;
        }
        return null;
      })();

    let plugins = list;
    if (!plugins) {
      try {
        plugins = await invoke<PluginListItem[]>("list_plugins");
        this.plugins = plugins;
      } catch (error) {
        console.error("prepareHookFrames list_plugins failed", error);
        return;
      }
    }

    const enabled = plugins.filter((plugin) => plugin.enabled);
    this.enabledPluginIds = new Set(enabled.map((plugin) => plugin.id));
    const keep = new Set(enabled.map((plugin) => plugin.id));
    for (const [pluginId, hook] of this.hookFrames) {
      if (keep.has(pluginId)) {
        continue;
      }
      hook.frame.remove();
      this.hookFrames.delete(pluginId);
    }

    for (const plugin of enabled) {
      if (this.hookFrames.has(plugin.id)) {
        continue;
      }
      await this.ensureHookFrame(plugin);
    }
  }

  private async ensureHookFrame(plugin: PluginListItem) {
    if (!this.elements || this.hookFrames.has(plugin.id)) {
      return;
    }

    let pluginHtml: string | null = null;
    try {
      pluginHtml = await invoke<string | null>("plugin_view_html", {
        payload: { pluginId: plugin.id },
      });
    } catch (error) {
      console.error("hook plugin_view_html failed", error);
      return;
    }
    if (!pluginHtml || !pluginHtml.trim()) {
      return;
    }

    const frame = document.createElement("iframe");
    frame.className = "hook-frame";
    frame.setAttribute("sandbox", "allow-scripts");
    frame.srcdoc = pluginHtml;
    this.elements.hookHost.appendChild(frame);

    const hook: HookFrame = {
      pluginId: plugin.id,
      pluginName: plugin.name,
      frame,
      ready: false,
    };
    this.hookFrames.set(plugin.id, hook);
    frame.addEventListener("load", () => {
      const target = this.hookFrames.get(plugin.id);
      if (!target) {
        return;
      }
      target.ready = true;
    });
  }

  private async collectPluginHookResults(query: string): Promise<SearchItem[]> {
    const trimmed = query.trim();
    if (!trimmed || this.manageMode || this.activePlugin) {
      return [];
    }

    await this.prepareHookFrames();
    const readyFrames = Array.from(this.hookFrames.values()).filter(
      (frame) => frame.ready && this.enabledPluginIds.has(frame.pluginId),
    );
    if (readyFrames.length === 0) {
      return [];
    }

    const token = ++this.hookToken;
    const expected = new Set(readyFrames.map((frame) => frame.pluginId));
    this.hookExpected.set(token, expected);
    this.hookBuckets.set(token, []);

    return await new Promise<SearchItem[]>((resolve) => {
      const timeout = window.setTimeout(() => {
        const bucket = this.hookBuckets.get(token) ?? [];
        this.hookExpected.delete(token);
        this.hookBuckets.delete(token);
        this.hookResolvers.delete(token);
        resolve(bucket);
      }, 220);

      this.hookResolvers.set(token, (items) => {
        window.clearTimeout(timeout);
        this.hookExpected.delete(token);
        this.hookBuckets.delete(token);
        resolve(items);
      });

      for (const frame of readyFrames) {
        frame.frame.contentWindow?.postMessage(
          {
            type: "jtools-hook-search",
            token,
            query: trimmed,
          },
          "*",
        );
      }
    });
  }

  private async openManageCenter(section: ManageSection = "plugins") {
    this.manageMode = true;
    this.manageSection = section;
    await this.refreshManageData();
    await this.render();
  }

  private async closeManageCenter() {
    this.manageMode = false;
    await this.loadStatus();
    if (this.query.trim()) {
      await this.performSearch(this.query.trim());
    } else {
      this.hasSearched = false;
      this.results = [];
      this.selectedIndex = 0;
    }
    await this.render();
    this.elements?.input.focus();
  }

  private async hideToTray() {
    try {
      await invoke("hide_launcher_to_tray");
    } catch (error) {
      console.error("hide_launcher_to_tray failed", error);
    }
  }

  private shouldEscapeHideToTray() {
    return this.preferences?.hideOnBlur ?? true;
  }

  private scheduleSearch(rawQuery: string) {
    this.query = rawQuery;
    if (this.searchTimer !== null) {
      window.clearTimeout(this.searchTimer);
    }

    if (this.manageMode) {
      return;
    }

    if (this.activePlugin) {
      this.pendingPluginQuery = rawQuery.trim();
      this.searchTimer = window.setTimeout(() => {
        this.pushQueryToPlugin(this.pendingPluginQuery);
      }, 40);
      return;
    }

    const query = rawQuery.trim();
    if (!query) {
      this.hasSearched = false;
      this.results = [];
      this.selectedIndex = 0;
      void (async () => {
        await this.render();
      })();
      return;
    }

    this.searchTimer = window.setTimeout(async () => {
      await this.performSearch(query);
      await this.render();
    }, 70);
  }

  private async performSearch(query: string) {
    const token = ++this.searchToken;
    let rawResults: SearchItem[] = [];
    let hookResults: SearchItem[] = [];
    try {
      const [coreResults, pluginHookResults] = await Promise.all([
        invoke<SearchItem[]>("search", { query }),
        this.collectPluginHookResults(query),
      ]);
      rawResults = coreResults;
      hookResults = pluginHookResults;
    } catch (error) {
      console.error("search failed", error);
    }

    if (token !== this.searchToken) {
      return;
    }

    this.hasSearched = true;
    const merged = [...rawResults, ...hookResults].filter((item) => {
      if (!item.plugin_id) {
        return true;
      }
      return this.enabledPluginIds.has(item.plugin_id);
    });
    this.results = this.dedupeResults(merged).sort((left, right) => {
      if (right.score !== left.score) {
        return right.score - left.score;
      }
      return left.title.localeCompare(right.title, "zh-CN");
    });
    this.selectedIndex =
      this.results.length === 0 ? 0 : Math.min(this.selectedIndex, this.results.length - 1);
  }

  private async openPluginWindow(pluginId: string, pluginName: string, query: string) {
    const normalizedPluginId = pluginId.trim();
    if (!normalizedPluginId) {
      return false;
    }
    const resolvedName = pluginName.trim() || "插件";
    const params = new URLSearchParams({
      mode: "plugin",
      pluginId: normalizedPluginId,
      pluginName: resolvedName,
      query,
    });
    const label = `plugin-${normalizedPluginId}-${Date.now()}`;
    const webview = new WebviewWindow(label, {
      url: `index.html?${params.toString()}`,
      title: `${resolvedName} · jtools`,
      width: 1120,
      height: 760,
      minWidth: 720,
      minHeight: 480,
      center: true,
      alwaysOnTop: false,
      decorations: true,
      minimizable: true,
      maximizable: true,
      resizable: true,
    });

    return await new Promise<boolean>((resolve) => {
      webview.once("tauri://created", () => {
        void (async () => {
          await this.applyPluginWindowIcon(webview, normalizedPluginId);
          resolve(true);
        })();
      });
      webview.once("tauri://error", (error) => {
        console.error("create plugin window failed", error);
        resolve(false);
      });
    });
  }

  private async openActivePluginInNewWindow() {
    if (!this.activePlugin) {
      return;
    }
    const opened = await this.openPluginWindow(
      this.activePlugin.id,
      this.activePlugin.name,
      this.query.trim(),
    );
    if (opened) {
      await this.clearActivePlugin();
    }
  }

  private async applyPluginWindowIcon(webview: WebviewWindow, pluginId: string) {
    try {
      const icon = await invoke<number[] | null>("plugin_window_icon_bytes", {
        payload: { pluginId },
      });
      if (!icon || icon.length === 0) {
        return;
      }
      await webview.setIcon(icon);
    } catch (error) {
      console.error("set plugin window icon failed", error);
    }
  }

  private async activatePlugin(item: SearchItem) {
    const pluginId = this.resolvePluginId(item);
    if (!pluginId) {
      return;
    }
    const seedQuery = this.query.trim();

    let pluginName: string | null = null;
    let pluginOptions: PluginViewOptions | null = null;
    try {
      pluginName = await invoke<string | null>("plugin_display_name", {
        payload: { pluginId },
      });
    } catch (error) {
      console.error("plugin_display_name failed", error);
    }
    try {
      pluginOptions = await invoke<PluginViewOptions | null>("plugin_view_options", {
        payload: { pluginId },
      });
    } catch (error) {
      console.error("plugin_view_options failed", error);
    }
    const fallbackName = this.fallbackPluginName(item);
    const showSearchInput = pluginOptions?.showSearchInput ?? true;

    this.activePlugin = {
      id: pluginId,
      name: (pluginName?.trim() || fallbackName).trim(),
      showSearchInput,
    };
    this.query = seedQuery;
    this.pendingPluginQuery = seedQuery;
    this.hasSearched = false;
    this.results = [];
    this.selectedIndex = 0;
    this.searchToken += 1;

    if (this.elements) {
      this.elements.input.value = seedQuery;
      this.elements.pluginFrame.style.height = `${this.pluginViewportMinHeight}px`;
      if (showSearchInput) {
        this.elements.input.focus();
      }
    }

    await this.loadPluginView();
    await this.render();
    this.pushQueryToPlugin(this.pendingPluginQuery);
  }

  private async loadPluginView() {
    if (!this.activePlugin || !this.elements) {
      return;
    }

    this.pluginFrameReady = false;
    let pluginHtml: string | null = null;
    try {
      pluginHtml = await invoke<string | null>("plugin_view_html", {
        payload: { pluginId: this.activePlugin.id },
      });
    } catch (error) {
      console.error("plugin_view_html failed", error);
    }

    if (!pluginHtml || !pluginHtml.trim()) {
      this.elements.pluginFrame.srcdoc = "";
      this.elements.pluginHostEmpty.classList.remove("hidden");
      return;
    }

    this.elements.pluginHostEmpty.classList.add("hidden");
    this.elements.pluginFrame.srcdoc = pluginHtml;
  }

  private async clearActivePlugin() {
    this.activePlugin = null;
    this.query = "";
    this.pendingPluginQuery = "";
    this.hasSearched = false;
    this.results = [];
    this.selectedIndex = 0;
    this.searchToken += 1;
    this.pluginFrameReady = false;

    if (this.elements) {
      this.elements.input.value = "";
      this.elements.input.focus();
      this.elements.pluginFrame.srcdoc = "";
      this.elements.pluginFrame.style.height = `${this.pluginViewportMinHeight}px`;
      this.elements.pluginHostEmpty.classList.add("hidden");
    }

    await this.render();
  }

  private pushQueryToPlugin(query: string) {
    if (!this.activePlugin || !this.elements || !this.pluginFrameReady) {
      return;
    }
    this.elements.pluginFrame.contentWindow?.postMessage(
      {
        type: "jtools-host-query",
        query,
      },
      "*",
    );
  }

  private submitPluginQuery() {
    if (!this.activePlugin || !this.elements || !this.pluginFrameReady) {
      return;
    }
    this.elements.pluginFrame.contentWindow?.postMessage(
      {
        type: "jtools-host-submit",
        query: this.query.trim(),
      },
      "*",
    );
  }

  private resolvePluginId(item: SearchItem): string | null {
    if (item.plugin_id) {
      return item.plugin_id;
    }
    if ("plugin_command" in item.action) {
      return item.action.plugin_command.plugin_id;
    }
    return null;
  }

  private fallbackPluginName(item: SearchItem): string {
    const [name] = item.subtitle.split("·");
    const trimmed = (name || "").trim();
    return trimmed || "插件";
  }

  private isPluginCommand(item: SearchItem): boolean {
    return "plugin_command" in item.action;
  }

  private dedupeResults(items: SearchItem[]): SearchItem[] {
    const deduped: SearchItem[] = [];
    const seen = new Set<string>();

    for (const item of items) {
      const key = this.actionKey(item.action);
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      deduped.push(item);
    }

    return deduped;
  }

  private actionKey(action: SearchAction): string {
    if ("host_command" in action) {
      return `host:${action.host_command}`;
    }
    if ("plugin_command" in action) {
      const payload = action.plugin_command;
      return `plugin:${payload.plugin_id}:${payload.command_id}:${payload.mode}`;
    }
    if ("open_url" in action) {
      return `url:${action.open_url}`;
    }
    return `copy:${action.copy_text}`;
  }

  private async execute(item: SearchItem | undefined) {
    if (!item) {
      return;
    }
    if ("host_command" in item.action) {
      if (item.action.host_command === "host.open_settings") {
        await this.openManageCenter("settings");
        return;
      }
      if (item.action.host_command === "host.reload_plugins") {
        await invoke<string>("reload_plugins");
        await this.loadStatus();
        await this.prepareHookFrames();
        await this.refreshManageData();
      } else {
        await invoke<string>("execute_item", { item });
      }
    } else if ("open_url" in item.action) {
      await invoke("capability_open_url", { url: item.action.open_url });
    } else if ("copy_text" in item.action) {
      await invoke("capability_copy_text", { text: item.action.copy_text });
      this.showToast("已复制到剪贴板");
    } else {
      await invoke<string>("execute_item", { item });
    }

    const query = this.query.trim();
    if (query) {
      await this.performSearch(query);
    } else {
      this.hasSearched = false;
      this.results = [];
      this.selectedIndex = 0;
    }
    await this.render();
  }

  private moveSelection(offset: number) {
    if (this.results.length === 0 || !this.hasSearched) {
      return;
    }
    const next = this.selectedIndex + offset;
    if (next < 0) {
      this.selectedIndex = this.results.length - 1;
      return;
    }
    if (next >= this.results.length) {
      this.selectedIndex = 0;
      return;
    }
    this.selectedIndex = next;
  }

  private async render() {
    if (!this.elements) {
      return;
    }

    this.elements.manageCenter.classList.toggle("hidden", !this.manageMode);
    this.elements.launcherBody.classList.toggle("hidden", this.manageMode);
    this.elements.manageOpenButton.classList.toggle("is-active", this.manageMode);
    this.elements.shell.classList.toggle("is-manage-mode", this.manageMode);

    if (this.manageMode) {
      this.renderManageCenter();
      await this.resizeWindowToContent();
      return;
    }

    if (this.activePlugin) {
      const showSearchInput = this.activePlugin.showSearchInput;
      this.elements.pluginContext.classList.remove("hidden");
      this.elements.pluginName.textContent = this.activePlugin.name;
      this.elements.searchBox.classList.toggle("hidden", !showSearchInput);
      if (showSearchInput) {
        this.elements.input.placeholder = `在 ${this.activePlugin.name} 中输入关键词`;
      }
      this.elements.pluginHost.classList.remove("hidden");
      this.elements.panel.classList.add("hidden");
      this.elements.results.innerHTML = "";
      this.elements.empty.hidden = true;
      this.elements.count.textContent = "0 项";
      await this.resizeWindowToContent();
      return;
    }

    this.elements.pluginContext.classList.add("hidden");
    this.elements.pluginName.textContent = "";
    this.elements.searchBox.classList.remove("hidden");
    this.elements.input.placeholder = "搜索插件、命令和动作";
    this.elements.pluginHost.classList.add("hidden");

    if (!this.hasSearched) {
      this.elements.panel.classList.add("hidden");
      this.elements.results.innerHTML = "";
      this.elements.empty.hidden = false;
      this.elements.count.textContent = "0 项";
      await this.resizeWindowToContent();
      return;
    }

    this.elements.panel.classList.remove("hidden");
    this.elements.count.textContent = `${this.results.length} 项`;

    if (this.results.length === 0) {
      this.elements.results.innerHTML = "";
      this.elements.empty.hidden = false;
      await this.resizeWindowToContent();
      return;
    }

    this.elements.empty.hidden = true;
    this.elements.results.innerHTML = this.results
      .map((item, index) => {
        const activeClass = index === this.selectedIndex ? " result-card-active" : "";
        const iconText = this.escapeHtml((item.title.trim().charAt(0) || "?").toUpperCase());
        return `
          <button class="result-card${activeClass}" type="button" data-result-index="${index}">
            <span class="result-icon">${iconText}</span>
            <div class="result-main">
              <span class="result-title">${this.escapeHtml(item.title)}</span>
              <span class="result-subtitle">${this.escapeHtml(item.subtitle)}</span>
            </div>
          </button>
        `;
      })
      .join("");

    this.elements.results.querySelectorAll<HTMLButtonElement>("[data-result-index]").forEach((button) => {
      button.addEventListener("click", async () => {
        const index = Number(button.dataset.resultIndex);
        const item = this.results[index];
        this.selectedIndex = index;
        if (item?.source_type === "plugin_hook" && "copy_text" in item.action) {
          await this.execute(item);
          return;
        }
        this.syncSelection();
      });
      button.addEventListener("dblclick", async () => {
        const index = Number(button.dataset.resultIndex);
        const item = this.results[index];
        this.selectedIndex = index;
        if (!item) {
          return;
        }
        if (this.isPluginCommand(item)) {
          await this.activatePlugin(item);
          return;
        }
        await this.execute(item);
      });
    });

    await this.resizeWindowToContent();
  }

  private renderManageCenter() {
    if (!this.elements) {
      return;
    }
    this.elements.shell
      .querySelectorAll<HTMLElement>(".manage-side-item[data-section]")
      .forEach((item) => {
        item.classList.toggle("is-active", item.dataset.section === this.manageSection);
      });

    if (this.manageSection === "plugins") {
      this.renderManagePluginsSection();
      return;
    }

    if (this.manageSection === "shortcuts") {
      this.renderManageShortcutsSection();
      return;
    }

    if (this.manageSection === "commands") {
      this.renderManageCommandsSection();
      return;
    }

    this.renderManageSettingsSection();
  }

  private renderManagePluginsSection() {
    if (!this.elements) {
      return;
    }

    const total = this.status?.totalPlugins ?? this.plugins.length;
    const enabled = this.status?.enabledPlugins ?? this.plugins.filter((plugin) => plugin.enabled).length;
    const disabled = this.status?.disabledPlugins ?? this.plugins.filter((plugin) => !plugin.enabled).length;
    const faulted = this.status?.faultedPlugins ?? 0;
    this.elements.manageContentBody.innerHTML = `
      <section class="manage-section">
        <div class="manage-section-head">
          <h3>插件状态</h3>
          <button data-manage-action="reload" class="manage-action manage-action-primary" type="button">重载插件</button>
        </div>
        <div class="manage-tools manage-tools-inline">
          <button data-manage-action="import-jtp" class="manage-action manage-action-secondary" type="button">导入插件包 .jtp</button>
          <button data-manage-action="pack-jtp-dialog" class="manage-action manage-action-secondary" type="button">选择目录并打包 .jtp</button>
          <button data-manage-action="download-template" class="manage-action manage-action-secondary" type="button">下载开发模板</button>
          <p class="manage-notice">${this.escapeHtml(this.manageNotice || "就绪")}</p>
        </div>
        <div class="manage-stats">
          <div class="stat"><span>TOTAL</span><strong>${total}</strong></div>
          <div class="stat"><span>ENABLED</span><strong>${enabled}</strong></div>
          <div class="stat"><span>DISABLED</span><strong>${disabled}</strong></div>
          <div class="stat"><span>FAULTED</span><strong>${faulted}</strong></div>
        </div>
      </section>
      <section class="manage-section manage-section-divider">
        <div class="manage-section-head">
          <h3>已安装插件</h3>
        </div>
        <div id="manage-plugins" class="manage-plugin-list">
          ${this.plugins
            .map((plugin) => {
              const icon = this.escapeHtml((plugin.name.charAt(0) || "?").toUpperCase());
              const name = this.escapeHtml(plugin.name);
              const desc = this.escapeHtml(plugin.description || "未提供描述");
              const count = `${plugin.commandCount} 个工具`;
              const switchClass = plugin.enabled ? "switch is-on" : "switch";
              const switchLabel = plugin.enabled ? "已启用" : "已禁用";
              return `
                <article class="manage-plugin-item">
                  <div class="manage-plugin-icon">${icon}</div>
                  <div class="manage-plugin-main">
                    <h4>${name}</h4>
                    <p>${desc}</p>
                    <span>${count}</span>
                  </div>
                  <div class="manage-plugin-actions">
                    <button class="${switchClass}" type="button" data-plugin-id="${plugin.id}" data-enabled="${plugin.enabled}" data-plugin-action="toggle" title="${switchLabel}">
                      <span></span>
                    </button>
                    <button class="remove-plugin" type="button" data-plugin-id="${plugin.id}" data-plugin-action="remove" title="卸载插件">卸载</button>
                  </div>
                </article>
              `;
            })
            .join("")}
        </div>
      </section>
    `;
    this.bindPluginManageActions();
  }

  private bindPluginManageActions() {
    if (!this.elements) {
      return;
    }
    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='reload']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          await invoke<string>("reload_plugins");
          await this.refreshManageData();
          this.manageNotice = "插件目录已重载。";
          await this.render();
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='download-template']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          try {
            const saved = await invoke<string>("download_plugin_template_dialog");
            this.manageNotice = `模板已生成: ${saved}`;
          } catch (error) {
            this.manageNotice = `模板下载失败: ${String(error)}`;
          }
          await this.render();
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='import-jtp']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          try {
            const message = await invoke<string>("install_plugin_from_jtp_dialog");
            this.manageNotice = message;
            await this.refreshManageData();
          } catch (error) {
            this.manageNotice = `导入失败: ${String(error)}`;
          }
          await this.render();
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='pack-jtp-dialog']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          try {
            const output = await invoke<string>("pack_jtp_from_dialog");
            this.manageNotice = `打包完成: ${output}`;
          } catch (error) {
            this.manageNotice = `打包失败: ${String(error)}`;
          }
          await this.render();
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-plugin-action='toggle']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          const pluginId = button.dataset.pluginId;
          const enabled = button.dataset.enabled === "true";
          if (!pluginId) {
            return;
          }
          try {
            await invoke("set_plugin_enabled", {
              payload: {
                pluginId,
                enabled: !enabled,
              },
            });
            if (this.activePlugin?.id === pluginId && enabled) {
              await this.clearActivePlugin();
            }
            await this.refreshManageData();
            this.manageNotice = `已${enabled ? "禁用" : "启用"}插件: ${pluginId}`;
            await this.render();
          } catch (error) {
            this.manageNotice = `切换失败: ${String(error)}`;
            await this.render();
          }
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-plugin-action='remove']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          const pluginId = button.dataset.pluginId;
          if (!pluginId) {
            return;
          }
          try {
            await invoke("uninstall_plugin", {
              payload: { pluginId },
            });
            if (this.activePlugin?.id === pluginId) {
              await this.clearActivePlugin();
            }
            await this.refreshManageData();
            this.manageNotice = `已卸载插件: ${pluginId}`;
            await this.render();
          } catch (error) {
            this.manageNotice = `卸载失败: ${String(error)}`;
            await this.render();
          }
        });
      });
  }

  private renderManageShortcutsSection() {
    if (!this.elements) {
      return;
    }
    const hotkey = this.preferences?.hotkey || this.status?.hotkey || "Alt+Space";
    this.elements.manageContentBody.innerHTML = `
      <section class="manage-section">
        <div class="manage-section-head">
          <h3>快捷方式</h3>
        </div>
        <div class="manage-shortcuts-grid">
          <article class="manage-info-card">
            <h4>全局唤起快捷键</h4>
            <p>在任意页面下呼出 jtools 启动器。</p>
            <strong class="manage-strong">${this.escapeHtml(hotkey)}</strong>
          </article>
          <article class="manage-info-card">
            <h4>托盘交互说明</h4>
            <p>左键托盘图标可显示启动器；按 Esc 键可隐藏到托盘。</p>
            <span class="manage-inline">建议开启“关闭窗口隐藏到托盘”。</span>
          </article>
        </div>
        <div class="manage-operation-block">
          <h4 class="manage-subtitle">测试动作</h4>
          <div class="manage-op-row">
            <button data-manage-action="hide-to-tray" class="manage-action manage-action-secondary" type="button">立即隐藏到托盘</button>
            <button data-manage-action="show-from-tray" class="manage-action manage-action-primary" type="button">立即显示启动器</button>
          </div>
          <p class="manage-notice">${this.escapeHtml(this.manageNotice || "就绪")}</p>
        </div>
      </section>
    `;

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='hide-to-tray']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          await this.hideToTray();
          this.manageNotice = "已隐藏到托盘，可点击托盘图标恢复。";
          await this.render();
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='show-from-tray']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          try {
            await invoke("show_launcher_from_tray");
            this.manageNotice = "已显示启动器。";
          } catch (error) {
            this.manageNotice = `显示失败: ${String(error)}`;
          }
          await this.render();
        });
      });
  }

  private renderManageCommandsSection() {
    if (!this.elements) {
      return;
    }

    const keyword = this.manageCommandFilter.trim().toLowerCase();
    const visible = this.manageCommands.filter((command) => {
      if (!keyword) {
        return true;
      }
      return (
        command.title.toLowerCase().includes(keyword) ||
        command.subtitle.toLowerCase().includes(keyword) ||
        (command.commandId || "").toLowerCase().includes(keyword) ||
        command.keywords.some((item) => item.toLowerCase().includes(keyword))
      );
    });

    this.elements.manageContentBody.innerHTML = `
      <section class="manage-section">
        <div class="manage-section-head">
          <h3>所有命令</h3>
          <span class="manage-count">${visible.length} 项</span>
        </div>
        <div class="manage-tools">
          <div class="manage-tool-row manage-tool-row-compact">
            <span>筛选</span>
            <input id="manage-command-filter" class="manage-input" type="text" value="${this.escapeHtml(this.manageCommandFilter)}" placeholder="输入标题、描述或关键词" />
          </div>
        </div>
        <div class="manage-command-list">
          ${
            visible.length === 0
              ? `<p class="manage-placeholder">没有匹配命令。</p>`
              : visible
                  .map((command) => {
                    const stateText = command.enabled ? "已启用" : "已禁用";
                    return `
                      <article class="manage-command-item ${command.enabled ? "" : "is-disabled"}">
                        <div class="manage-command-main">
                          <h4>${this.escapeHtml(command.title)}</h4>
                          <p>${this.escapeHtml(command.subtitle)}</p>
                          <span>${this.escapeHtml(command.commandId || command.id)} · ${this.escapeHtml(command.mode || "action")} · ${stateText}</span>
                        </div>
                        <button
                          class="manage-action manage-action-secondary"
                          type="button"
                          data-command-id="${this.escapeHtml(command.id)}"
                          data-command-run="true"
                          ${command.enabled ? "" : "disabled"}
                        >执行</button>
                      </article>
                    `;
                  })
                  .join("")
          }
        </div>
      </section>
    `;

    const filter = this.elements.manageContentBody.querySelector<HTMLInputElement>("#manage-command-filter");
    if (filter) {
      filter.addEventListener("input", async () => {
        this.manageCommandFilter = filter.value;
        await this.render();
      });
    }

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-command-run='true']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          const commandId = button.dataset.commandId;
          if (!commandId) {
            return;
          }
          const command = this.manageCommands.find((item) => item.id === commandId);
          if (!command || !command.enabled) {
            return;
          }
          const item: SearchItem = {
            source_type: command.pluginId ? "plugin_command" : "host_command",
            source_id: command.commandId || command.id,
            plugin_id: command.pluginId,
            title: command.title,
            subtitle: command.subtitle,
            keywords: command.keywords,
            score: 100,
            action: command.action,
          };
          try {
            await this.execute(item);
            this.manageNotice = `已执行命令: ${command.title}`;
          } catch (error) {
            this.manageNotice = `执行失败: ${String(error)}`;
          }
          await this.render();
        });
      });
  }

  private renderManageSettingsSection() {
    if (!this.elements) {
      return;
    }
    const preferences = this.preferences;
    const maxResults = preferences?.maxResults ?? 8;
    const includeRecent = preferences?.includeRecent ?? true;
    const hideOnBlur = preferences?.hideOnBlur ?? true;
    const closeToTray = preferences?.closeToTray ?? true;
    const rootDir = preferences?.rootDir ?? this.status?.rootDir ?? "";
    const pluginsDir = preferences?.pluginsDir ?? "";

    this.elements.manageContentBody.innerHTML = `
      <section class="manage-section">
        <div class="manage-section-head">
          <h3>设置</h3>
        </div>
        <div class="manage-tools">
          <div class="manage-tool-row">
            <span>最大搜索结果</span>
            <input id="setting-max-results" class="manage-input small" type="number" min="1" max="50" value="${maxResults}" />
          </div>
          <div class="manage-tool-row manage-tool-row-check">
            <span>最近使用参与搜索</span>
            <label class="manage-check"><input id="setting-include-recent" type="checkbox" ${includeRecent ? "checked" : ""} />启用</label>
          </div>
          <div class="manage-tool-row manage-tool-row-check">
            <span>Esc 键隐藏到托盘</span>
            <label class="manage-check"><input id="setting-hide-on-blur" type="checkbox" ${hideOnBlur ? "checked" : ""} />启用</label>
          </div>
          <div class="manage-tool-row manage-tool-row-check">
            <span>关闭窗口隐藏到托盘</span>
            <label class="manage-check"><input id="setting-close-to-tray" type="checkbox" ${closeToTray ? "checked" : ""} />启用</label>
          </div>
          <div class="manage-tool-row">
            <span>运行目录</span>
            <code class="manage-path-code">${this.escapeHtml(rootDir)}</code>
          </div>
          <div class="manage-tool-row">
            <span>插件目录</span>
            <code class="manage-path-code">${this.escapeHtml(pluginsDir)}</code>
          </div>
          <div class="manage-tool-row">
            <span>设置操作</span>
            <button data-manage-action="save-settings" class="manage-action manage-action-primary" type="button">保存设置</button>
            <button data-manage-action="reload-settings" class="manage-action manage-action-secondary" type="button">刷新数据</button>
          </div>
          <p class="manage-notice">${this.escapeHtml(this.manageNotice || "就绪")}</p>
        </div>
      </section>
    `;

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='save-settings']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          const maxInput = this.elements?.manageContentBody.querySelector<HTMLInputElement>("#setting-max-results");
          const includeRecentInput = this.elements?.manageContentBody.querySelector<HTMLInputElement>("#setting-include-recent");
          const hideOnBlurInput = this.elements?.manageContentBody.querySelector<HTMLInputElement>("#setting-hide-on-blur");
          const closeToTrayInput = this.elements?.manageContentBody.querySelector<HTMLInputElement>("#setting-close-to-tray");
          const maxValue = Number(maxInput?.value || "8");
          try {
            const updated = await invoke<UserPreferences>("update_user_preferences", {
              payload: {
                maxResults: Number.isFinite(maxValue) ? maxValue : 8,
                includeRecent: includeRecentInput?.checked ?? true,
                hideOnBlur: hideOnBlurInput?.checked ?? true,
                closeToTray: closeToTrayInput?.checked ?? true,
              },
            });
            this.preferences = updated;
            await this.loadStatus();
            this.manageNotice = "设置已保存。";
          } catch (error) {
            this.manageNotice = `保存失败: ${String(error)}`;
          }
          await this.render();
        });
      });

    this.elements.manageContentBody
      .querySelectorAll<HTMLButtonElement>("[data-manage-action='reload-settings']")
      .forEach((button) => {
        button.addEventListener("click", async () => {
          await this.refreshManageData();
          this.manageNotice = "设置已刷新。";
          await this.render();
        });
      });
  }

  private syncSelection() {
    if (!this.elements) {
      return;
    }
    this.elements.results.querySelectorAll<HTMLButtonElement>("[data-result-index]").forEach((button) => {
      const index = Number(button.dataset.resultIndex);
      button.classList.toggle("result-card-active", index === this.selectedIndex);
    });
  }

  private scheduleFocusRecovery() {
    if (!this.elements || this.manageMode) {
      return;
    }
    if (this.isInputComposing) {
      return;
    }
    this.clearFocusRecoveryTimers();
    const recover = () => {
      this.focusSearchInput();
    };
    recover();
    for (const delay of [24, 80, 180, 320]) {
      const timer = window.setTimeout(() => {
        recover();
      }, delay);
      this.focusRecoveryTimers.push(timer);
    }
  }

  private clearFocusRecoveryTimers() {
    for (const timer of this.focusRecoveryTimers) {
      window.clearTimeout(timer);
    }
    this.focusRecoveryTimers = [];
  }

  private focusSearchInput() {
    if (!this.elements || this.manageMode) {
      return;
    }
    if (this.isInputComposing) {
      return;
    }
    const input = this.elements.input;
    if (document.activeElement !== input) {
      input.focus();
    }
  }

  private async resizeWindowToContent() {
    if (!this.elements) {
      return;
    }

    await new Promise<void>((resolve) => {
      window.requestAnimationFrame(() => resolve());
    });

    if (this.manageMode) {
      await this.setWindowSize(this.managerHeight, this.managerWidth);
      return;
    }

    const target = this.computeTargetHeight();
    const height = Math.max(this.minWindowHeight, Math.min(this.maxWindowHeight, target));
    await this.setWindowSize(height, this.launcherWidth);
  }

  private computeTargetHeight() {
    if (!this.elements) {
      return this.minWindowHeight;
    }
    return Math.max(this.minWindowHeight, this.elements.shell.scrollHeight + 2);
  }

  private async setWindowSize(height: number, width: number) {
    const roundedHeight = Math.ceil(height);
    const roundedWidth = Math.ceil(width);
    const sameHeight = Math.abs(roundedHeight - this.lastAppliedHeight) < 1;
    const sameWidth = Math.abs(roundedWidth - this.lastAppliedWidth) < 1;
    if (sameHeight && sameWidth) {
      return;
    }
    try {
      await invoke("resize_launcher_window", { height: roundedHeight, width: roundedWidth });
      this.lastAppliedHeight = roundedHeight;
      this.lastAppliedWidth = roundedWidth;
    } catch (error) {
      console.error("resize_launcher_window failed", error);
    }
  }

  private escapeHtml(value: string) {
    return value
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }

  private showToast(message: string, durationMs = 1400) {
    if (!this.elements) {
      return;
    }
    this.elements.toast.textContent = message;
    this.elements.toast.classList.remove("hidden");
    if (this.toastTimer !== null) {
      window.clearTimeout(this.toastTimer);
    }
    this.toastTimer = window.setTimeout(() => {
      if (!this.elements) {
        return;
      }
      this.elements.toast.classList.add("hidden");
      this.toastTimer = null;
    }, durationMs);
  }
}

class PluginWindowApp {
  private app = document.querySelector<HTMLDivElement>("#app");
  private pluginId = "";
  private pluginName = "插件";
  private initialQuery = "";
  private showSearchInput = true;
  private header: HTMLElement | null = null;
  private frame: HTMLIFrameElement | null = null;
  private input: HTMLInputElement | null = null;
  private empty: HTMLParagraphElement | null = null;
  private pluginFrameReady = false;

  constructor() {
    const params = new URLSearchParams(window.location.search);
    this.pluginId = (params.get("pluginId") || "").trim();
    this.pluginName = (params.get("pluginName") || "插件").trim() || "插件";
    this.initialQuery = (params.get("query") || "").trim();
  }

  async init() {
    if (!this.app) {
      throw new Error("App root not found");
    }

    this.mount();
    if (!this.pluginId) {
      this.showEmpty("未提供 pluginId，无法打开插件窗口。");
      return;
    }

    try {
      const resolved = await invoke<string | null>("plugin_display_name", {
        payload: { pluginId: this.pluginId },
      });
      if (resolved?.trim()) {
        this.pluginName = resolved.trim();
      }
    } catch (error) {
      console.error("plugin_display_name failed", error);
    }
    try {
      const options = await invoke<PluginViewOptions | null>("plugin_view_options", {
        payload: { pluginId: this.pluginId },
      });
      this.showSearchInput = options?.showSearchInput ?? true;
    } catch (error) {
      console.error("plugin_view_options failed", error);
    }

    this.updateTitle();
    if (this.input) {
      this.input.placeholder = `在 ${this.pluginName} 中输入关键词`;
      this.input.value = this.initialQuery;
    }
    this.applySearchInputVisibility();
    await this.loadPluginView();
  }

  private mount() {
    if (!this.app) {
      return;
    }

    this.app.innerHTML = `
      <main class="plugin-window-shell">
        <header class="plugin-window-header" id="plugin-window-header">
<!--          <h1 class="plugin-window-title" id="plugin-window-title">${this.escapeHtml(this.pluginName)}</h1>-->
          <input
            id="plugin-window-input"
            class="plugin-window-input"
            type="text"
            autocomplete="off"
            autocapitalize="off"
            autocorrect="off"
            spellcheck="false"
            placeholder="输入关键词"
          />
        </header>
        <section class="plugin-window-body">
          <iframe
            id="plugin-window-frame"
            class="plugin-window-frame"
            sandbox="allow-scripts allow-popups allow-forms allow-modals"
          ></iframe>
          <p class="plugin-window-empty hidden" id="plugin-window-empty">插件页面不可用</p>
        </section>
      </main>
    `;

    this.header = this.app.querySelector<HTMLElement>("#plugin-window-header");
    this.frame = this.app.querySelector<HTMLIFrameElement>("#plugin-window-frame");
    this.input = this.app.querySelector<HTMLInputElement>("#plugin-window-input");
    this.empty = this.app.querySelector<HTMLParagraphElement>("#plugin-window-empty");

    this.frame?.addEventListener("load", () => {
      this.pluginFrameReady = true;
      this.pushQueryToPlugin(this.input?.value.trim() || this.initialQuery);
    });

    this.input?.addEventListener("input", () => {
      this.pushQueryToPlugin(this.input?.value.trim() || "");
    });

    this.input?.addEventListener("keydown", (event) => {
      if (event.key !== "Enter") {
        return;
      }
      event.preventDefault();
      this.submitPluginQuery();
    });

    window.addEventListener("message", this.onPluginMessage);
    this.input?.focus();
  }

  private async loadPluginView() {
    if (!this.pluginId || !this.frame) {
      return;
    }

    this.pluginFrameReady = false;
    let pluginHtml: string | null = null;
    try {
      pluginHtml = await invoke<string | null>("plugin_view_html", {
        payload: { pluginId: this.pluginId },
      });
    } catch (error) {
      console.error("plugin_view_html failed", error);
    }

    if (!pluginHtml || !pluginHtml.trim()) {
      this.frame.srcdoc = "";
      this.showEmpty("插件页面不可用或插件已被禁用。");
      return;
    }

    if (this.empty) {
      this.empty.classList.add("hidden");
      this.empty.textContent = "插件页面不可用";
    }
    this.frame.srcdoc = pluginHtml;
  }

  private onPluginMessage = async (event: MessageEvent<unknown>) => {
    if (!this.frame || event.source !== this.frame.contentWindow) {
      return;
    }

    const data = event.data as PluginMessage | null;
    if (!data || typeof data !== "object" || !("type" in data)) {
      return;
    }

    if (data.type === "jtools-plugin-ready") {
      this.pluginFrameReady = true;
      this.pushQueryToPlugin(this.input?.value.trim() || this.initialQuery);
      return;
    }

    if (data.type === "jtools-plugin-action") {
      try {
        if (data.action === "open_url") {
          await invoke("capability_open_url", { url: data.url });
        } else if (data.action === "copy_text") {
          await invoke("capability_copy_text", { text: data.text });
        }
      } catch (error) {
        console.error("plugin action failed", error);
      }
      return;
    }

    if (data.type === "jtools-plugin-capability") {
      const requestId = typeof data.requestId === "string" ? data.requestId : "";
      try {
        const result = await invoke<PluginCapabilityResponse>("plugin_capability_call", {
          payload: {
            pluginId: this.pluginId,
            capability: data.capability,
            requestId,
            args: data.args ?? {},
          },
        });
        this.frame.contentWindow?.postMessage(
          {
            type: "jtools-host-capability-result",
            requestId: result.requestId ?? requestId,
            ok: result.ok,
            message: result.message,
            data: result.data ?? null,
          },
          "*",
        );
      } catch (error) {
        this.frame.contentWindow?.postMessage(
          {
            type: "jtools-host-capability-result",
            requestId,
            ok: false,
            message: String(error),
          },
          "*",
        );
      }
    }
  };

  private pushQueryToPlugin(query: string) {
    if (!this.frame || !this.pluginFrameReady || !this.showSearchInput) {
      return;
    }
    this.frame.contentWindow?.postMessage(
      {
        type: "jtools-host-query",
        query,
      },
      "*",
    );
  }

  private submitPluginQuery() {
    if (!this.frame || !this.pluginFrameReady || !this.showSearchInput) {
      return;
    }
    this.frame.contentWindow?.postMessage(
      {
        type: "jtools-host-submit",
        query: this.input?.value.trim() || "",
      },
      "*",
    );
  }

  private updateTitle() {
    document.title = `${this.pluginName} · jtools`;
    const title = this.app?.querySelector<HTMLElement>("#plugin-window-title");
    if (title) {
      title.textContent = this.pluginName;
    }
  }

  private applySearchInputVisibility() {
    this.header?.classList.toggle("hidden", !this.showSearchInput);
    if (!this.input) {
      return;
    }
    this.input.classList.toggle("hidden", !this.showSearchInput);
    if (!this.showSearchInput) {
      this.input.value = "";
      this.input.blur();
      this.initialQuery = "";
      return;
    }
    this.input.focus();
  }

  private showEmpty(message: string) {
    if (this.empty) {
      this.empty.textContent = message;
      this.empty.classList.remove("hidden");
    }
  }

  private escapeHtml(value: string) {
    return value
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;")
      .replaceAll("'", "&#39;");
  }
}

const mode = new URLSearchParams(window.location.search).get("mode");
if (mode === "plugin") {
  const app = new PluginWindowApp();
  void app.init();
} else {
  const app = new SearchApp();
  void app.init();
}
