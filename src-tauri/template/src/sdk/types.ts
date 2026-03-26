/**
 * 宿主返回的能力调用结果。
 */
export interface HostCapabilityResponse<T = unknown> {
  requestId?: string;
  ok: boolean;
  message: string;
  data?: T;
}

/**
 * 插件向 hook 搜索回传的单条结果。
 */
export interface HookResultItem {
  title: string;
  subtitle?: string;
  score?: number;
  action: Record<string, unknown>;
}

/**
 * 插件内部候选项（用于 view 模式列表执行）。
 */
export interface PluginCandidateItem {
  title: string;
  subtitle?: string;
  action: {
    capability: string;
    args?: Record<string, unknown>;
  };
}

/**
 * 宿主 -> 插件 message 事件结构。
 */
export type HostMessageData =
  | {
      type: "jtools-host-query";
      query?: string;
    }
  | {
      type: "jtools-host-submit";
    }
  | {
      type: "jtools-hook-search";
      query?: string;
      token?: number;
    }
  | {
      type: "jtools-host-capability-result";
      requestId?: string;
      ok?: boolean;
      message?: string;
      data?: unknown;
    }
  | Record<string, unknown>;
