# jtools 插件模板（Vite + Vue3 + TypeScript）

这个模板已按插件开发场景做了两件关键优化：

- SDK 改为 `src/sdk` 下的 TypeScript 工具包，支持类型提示
- `.jtp` 打包改为“根目录平铺”结构，不再是 `dist/` 套壳
- 构建默认启用 `vite-plugin-singlefile`，避免 `srcdoc` 场景下本地资源加载受限

---

## 1. 目录结构

```text
.
├─ manifest.json
├─ package.json
├─ tsconfig.json
├─ vite.config.js
├─ index.html
├─ src/
│  ├─ main.ts
│  ├─ App.vue
│  ├─ vite-env.d.ts
│  └─ sdk/
│     ├─ types.ts
│     └─ jtools.ts
├─ scripts/
│  └─ build-jtp.mjs
└─ README.md
```

---

## 2. SDK 使用方式（TS）

在组件里直接 import：

```ts
import { jtools } from "./sdk/jtools";
```

能力调用示例：

```ts
await jtools.capabilities.copyText("hello");
await jtools.capabilities.cacheSet("k", { a: 1 });
```

搜索 hook 回包示例：

```ts
jtools.postHookResults(token, [
  { title: "示例", subtitle: "hook", score: 90, action: { copy_text: "abc" } },
]);
```

---

## 3. 开发命令

安装依赖：

```bash
bun install
```

开发：

```bash
bun run dev
```

构建：

```bash
bun run build
```

构建并打包 jtp：

```bash
bun run build:jtp
```

---

## 4. jtp 打包规则（已改）

`scripts/build-jtp.mjs` 会：

1. 检查 `dist/index.html` 是否存在
2. 打包 `manifest.json`
3. 将 `dist/**` 平铺到 jtp 根目录
4. 输出到 `release/<plugin-id>-<version>.jtp`

即 jtp 内部结构会是：

```text
manifest.json
index.html
assets/...
```

---

## 5. 清单与约定

1. `manifest.entry` 必须是 `index.html`
2. `App.vue` 里的 `PLUGIN_ID` 必须与 `manifest.id` 一致
3. 能力调用前必须在清单声明对应 `permissions`

---

## 6. 提权边界

框架不自动提权。  
若插件要执行高权限动作（如敏感系统写入），需要插件自己触发 UAC/sudo 流程并提示用户确认。
