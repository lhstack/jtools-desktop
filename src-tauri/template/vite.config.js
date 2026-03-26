import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { viteSingleFile } from "vite-plugin-singlefile";

// 使用相对 base，保证 dist/index.html 在插件目录中被宿主加载时，
// JS/CSS 静态资源能按相对路径正确解析。
export default defineConfig({
  base: "./",
  plugins: [vue(), viteSingleFile()],
  build: {
    cssCodeSplit: false,
    assetsInlineLimit: 1024 * 1024 * 20,
  },
});
