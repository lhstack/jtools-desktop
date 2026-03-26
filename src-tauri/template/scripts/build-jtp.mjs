import fs from "node:fs/promises";
import path from "node:path";
import JSZip from "jszip";

/**
 * jtp 打包脚本（自动化）
 *
 * 目标：
 * 1) 读取 manifest.json 的 id/version
 * 2) 校验 dist/index.html 是否存在（必须先 vite build）
 * 3) 将 dist 内容平铺到 jtp 根目录（避免出现 dist 套壳）
 * 4) 仅打包运行所需文件（manifest.json + build 产物 + 可选 icon）
 * 4) 输出 release/<id>-<version>.jtp
 */

const cwd = process.cwd();
const manifestPath = path.join(cwd, "manifest.json");
const distDir = path.join(cwd, "dist");
const distEntry = path.join(distDir, "index.html");
const releaseDir = path.join(cwd, "release");

function sanitizeSegment(value) {
  return String(value || "")
    .replace(/[\\/:*?"<>|]/g, "_")
    .trim();
}

async function exists(target) {
  try {
    await fs.access(target);
    return true;
  } catch {
    return false;
  }
}

async function addFile(zip, absolutePath, relativePath) {
  const content = await fs.readFile(absolutePath);
  zip.file(relativePath.replace(/\\/g, "/"), content);
}

async function addDirectory(zip, absoluteDir, relativeDir) {
  const entries = await fs.readdir(absoluteDir, { withFileTypes: true });
  for (const entry of entries) {
    const abs = path.join(absoluteDir, entry.name);
    const rel = relativeDir ? path.join(relativeDir, entry.name) : entry.name;
    if (entry.isDirectory()) {
      await addDirectory(zip, abs, rel);
      continue;
    }
    if (entry.isFile()) {
      await addFile(zip, abs, rel);
    }
  }
}

async function readManifest() {
  if (!(await exists(manifestPath))) {
    throw new Error("缺少 manifest.json，无法打包 jtp");
  }
  const content = await fs.readFile(manifestPath, "utf8");
  const manifest = JSON.parse(content);

  if (!String(manifest.id || "").trim()) {
    throw new Error("manifest.id 不能为空");
  }
  if (!String(manifest.version || "").trim()) {
    throw new Error("manifest.version 不能为空");
  }
  if (!String(manifest.entry || "").trim()) {
    throw new Error("manifest.entry 不能为空");
  }
  return manifest;
}

async function main() {
  const manifest = await readManifest();

  if (!(await exists(distEntry))) {
    throw new Error("未找到 dist/index.html，请先执行 `npm run build` 或 `bun run build`");
  }

  const zip = new JSZip();

  // 1) 固定打包 manifest.json
  await addFile(zip, manifestPath, "manifest.json");
  // 2) 将 dist 全量产物打包到根目录（而不是 dist/ 子目录）
  await addDirectory(zip, distDir, "");

  // 3) 可选：打包 icon（如果 manifest.icon 指向构建产物之外文件）
  if (typeof manifest.icon === "string" && manifest.icon.trim()) {
    const iconPath = path.join(cwd, manifest.icon.trim());
    if (await exists(iconPath)) {
      const stat = await fs.stat(iconPath);
      if (stat.isFile()) {
        await addFile(zip, iconPath, manifest.icon.trim());
      }
    }
  }

  await fs.mkdir(releaseDir, { recursive: true });

  const id = sanitizeSegment(manifest.id);
  const version = sanitizeSegment(manifest.version);
  const output = path.join(releaseDir, `${id}-${version}.jtp`);

  const data = await zip.generateAsync({
    type: "nodebuffer",
    compression: "DEFLATE",
    compressionOptions: { level: 9 },
  });
  await fs.writeFile(output, data);

  console.log(`[jtp] 打包完成: ${output}`);
}

main().catch((error) => {
  console.error(`[jtp] 打包失败: ${error.message || String(error)}`);
  process.exit(1);
});
