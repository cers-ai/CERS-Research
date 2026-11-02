#!/usr/bin/env node
/**
 * 从品牌 SVG 生成 Windows/macOS/Linux/Android 全套图标。
 * 源文件：assets/app-icon.svg；生成命令：npm run icons
 */

import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = resolve(root, "assets/app-icon.svg");
const output = resolve(root, "apps/desktop/src-tauri/icons");

const result = spawnSync(
  process.platform === "win32" ? "npx.cmd" : "npx",
  ["tauri", "icon", source, "-o", output],
  { cwd: root, stdio: "inherit" },
);

if (result.error) {
  console.error(result.error);
  process.exit(1);
}
process.exit(result.status ?? 1);
