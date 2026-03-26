import type { HookResultItem, HostCapabilityResponse } from "./types";

/**
 * jtools SDK（TypeScript 模块版）
 *
 * 目标：
 * 1) 用 TS 类型约束能力调用参数和返回结构
 * 2) 提供 Promise 风格调用（自动 requestId 关联回包）
 * 3) 作为 src/sdk 工具包直接 import 使用
 */

type PendingTask = {
  resolve: (value: HostCapabilityResponse<unknown>) => void;
  reject: (reason?: unknown) => void;
};

class JToolsSdk {
  private seq = 0;
  private pluginId = "demo-plugin-vue";
  private pending = new Map<string, PendingTask>();
  private listenerAttached = false;

  configure(options: { pluginId?: string } = {}): void {
    if (typeof options.pluginId === "string" && options.pluginId.trim()) {
      this.pluginId = options.pluginId.trim();
    }
    this.attachListenerOnce();
  }

  /**
   * 宿主能力通用调用入口。
   */
  call<T = unknown>(
    capability: string,
    args: Record<string, unknown> = {},
    timeoutMs = 8000,
  ): Promise<HostCapabilityResponse<T>> {
    this.attachListenerOnce();
    const requestId = this.nextRequestId();

    return new Promise<HostCapabilityResponse<T>>((resolve, reject) => {
      this.pending.set(requestId, {
        resolve: (value) => resolve(value as HostCapabilityResponse<T>),
        reject,
      });
      window.parent.postMessage(
        {
          type: "jtools-plugin-capability",
          requestId,
          capability,
          args,
        },
        "*",
      );

      window.setTimeout(() => {
        const task = this.pending.get(requestId);
        if (!task) return;
        this.pending.delete(requestId);
        task.reject(new Error(`capability timeout: ${capability}`));
      }, timeoutMs);
    });
  }

  /**
   * 通知宿主调整插件视图高度。
   */
  setHeight(height: number): void {
    window.parent.postMessage({ type: "jtools-plugin-height", height }, "*");
  }

  /**
   * 向宿主回传 hook 搜索结果。
   */
  postHookResults(token: number, items: HookResultItem[]): void {
    window.parent.postMessage(
      {
        type: "jtools-hook-results",
        pluginId: this.pluginId,
        token: Number(token || 0),
        items,
      },
      "*",
    );
  }

  get capabilities() {
    return {
      openUrl: (url: string) => this.call("open_url", { url }),
      copyText: (text: string) => this.call("copy_text", { text }),
      readClipboard: () => this.call<{ text: string }>("read_clipboard"),
      hideToTray: () => this.call("hide_to_tray"),
      showLauncher: () => this.call("show_launcher"),
      reloadPlugins: () => this.call("reload_plugins"),

      execShell: (command: string, cwd?: string) =>
        this.call<{
          success: boolean;
          code: number | null;
          stdout: string;
          stderr: string;
        }>("exec_shell", { command, cwd }),

      fileReadPath: (path: string) =>
        this.call<{ path: string; content: string }>("file_read_path", { path }),
      fileWritePath: (path: string, content: string) =>
        this.call("file_write_path", { path, content }),
      fileAppendPath: (path: string, content: string) =>
        this.call("file_append_path", { path, content }),
      fileRemovePath: (path: string) => this.call("file_remove_path", { path }),

      fileReadText: (path: string) =>
        this.call<{ path: string; content: string }>("file_read_text", { path }),
      fileWriteText: (path: string, content: string) =>
        this.call("file_write_text", { path, content }),
      fileAppendText: (path: string, content: string) =>
        this.call("file_append_text", { path, content }),
      fileCreateDir: (path: string) => this.call("file_create_dir", { path }),
      fileListDir: (path = "") =>
        this.call<{ path: string; items: Array<Record<string, unknown>> }>(
          "file_list_dir",
          { path },
        ),
      fileExists: (path: string) =>
        this.call<{ path: string; exists: boolean }>("file_exists", { path }),
      fileRemove: (path: string) => this.call("file_remove", { path }),

      cacheGet: (key: string) =>
        this.call<{ key: string; exists: boolean; value: unknown }>("cache_get", { key }),
      cacheSet: (key: string, value: unknown) => this.call("cache_set", { key, value }),
      cacheDelete: (key: string) => this.call("cache_delete", { key }),
      cacheListKeys: () => this.call<{ keys: string[] }>("cache_list_keys"),
      cacheClear: () => this.call("cache_clear"),
    };
  }

  private nextRequestId(): string {
    this.seq += 1;
    return `req-${Date.now()}-${this.seq}`;
  }

  /**
   * 只注册一次 message listener，负责把宿主能力回包派发给对应 Promise。
   */
  private attachListenerOnce(): void {
    if (this.listenerAttached) return;
    this.listenerAttached = true;

    window.addEventListener("message", (event) => {
      const data = (event.data ?? {}) as Record<string, unknown>;
      if (data.type !== "jtools-host-capability-result") return;

      const requestId = String(data.requestId ?? "");
      const task = this.pending.get(requestId);
      if (!task) return;
      this.pending.delete(requestId);

      const payload: HostCapabilityResponse = {
        requestId,
        ok: Boolean(data.ok),
        message: String(data.message ?? ""),
        data: data.data,
      };
      if (payload.ok) {
        task.resolve(payload);
      } else {
        task.reject(new Error(payload.message || "capability failed"));
      }
    });
  }
}

export const jtools = new JToolsSdk();
