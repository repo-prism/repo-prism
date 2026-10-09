#!/usr/bin/env node
// 版本一致性校验 —— 发布硬化的一部分。
//
// 为什么需要它：版本号在本项目里有**三处**独立声明，谁都能单独改。
// 只改一处就会出一个「界面显示 0.2.0、安装包元数据写 0.1.0」的版本，
// 而且直到用户装完才发现。这个脚本把三处绑成一个断言。
//
//   1. package.json                    → version
//   2. Cargo.toml（workspace）          → [workspace.package] version
//   3. src-tauri/tauri.conf.json       → version
//
// 其余 crate 的 Cargo.toml 写的是 `version.workspace = true`，自动继承，
// 因此不需要单独校验 —— 但脚本会顺带确认它们确实是继承而非硬编码。
//
// 用法：
//   node scripts/check-versions.mjs                      # 只校验三处一致
//   node scripts/check-versions.mjs --expect 0.2.0       # 还要求等于指定版本
//   node scripts/check-versions.mjs --expect-ref v0.2.0  # 用 tag 校验（自动去 v）

import { readFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** @param {string} relativePath */
function read(relativePath) {
  const full = join(root, relativePath);
  if (!existsSync(full)) {
    throw new Error(`缺少文件：${relativePath}`);
  }
  return readFileSync(full, "utf8");
}

/** 取出 Cargo.toml 中 `[workspace.package]` 段的 version。 */
function workspaceCargoVersion() {
  const text = read("Cargo.toml");
  const lines = text.split("\n");
  let inside = false;
  for (const raw of lines) {
    const line = raw.trim();
    if (line.startsWith("[")) {
      inside = line === "[workspace.package]";
      continue;
    }
    if (!inside) continue;
    const match = /^version\s*=\s*"([^"]+)"/.exec(line);
    if (match) return match[1];
  }
  throw new Error("Cargo.toml 里找不到 [workspace.package] 段的 version");
}

/** 确认各 crate 走的是继承而不是硬编码版本。 */
function checkInheritance() {
  const problems = [];
  const dirs = ["crates", "src-tauri"];
  for (const dir of dirs) {
    const base = join(root, dir);
    if (!existsSync(base)) continue;
    for (const name of readdirSync(base)) {
      const manifest = join(base, name, "Cargo.toml");
      if (!existsSync(manifest)) continue;
      const text = readFileSync(manifest, "utf8");
      if (/^version\s*=\s*"/m.test(text)) {
        problems.push(`${dir}/${name}/Cargo.toml 硬编码了 version，应改为 version.workspace = true`);
      }
    }
  }
  return problems;
}

function parseArgs(argv) {
  const options = { expect: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--expect") {
      options.expect = argv[i + 1] ?? null;
      i += 1;
    } else if (arg === "--expect-ref") {
      const ref = argv[i + 1] ?? "";
      options.expect = ref.replace(/^refs\/tags\//, "").replace(/^v/, "");
      i += 1;
    } else if (arg.startsWith("--expect=")) {
      options.expect = arg.slice("--expect=".length);
    } else {
      throw new Error(`无法识别的参数：${arg}`);
    }
  }
  return options;
}

function main() {
  const options = parseArgs(process.argv.slice(2));

  const sources = [
    { label: "package.json", path: "package.json", version: JSON.parse(read("package.json")).version },
    { label: "Cargo.toml [workspace.package]", path: "Cargo.toml", version: workspaceCargoVersion() },
    {
      label: "src-tauri/tauri.conf.json",
      path: "src-tauri/tauri.conf.json",
      version: JSON.parse(read("src-tauri/tauri.conf.json")).version,
    },
  ];

  const failures = [];

  const distinct = new Set(sources.map((s) => s.version));
  if (distinct.size !== 1) {
    failures.push("三处版本号不一致：");
    for (const source of sources) {
      failures.push(`  ${source.version.padEnd(12)} ${source.path}`);
    }
  }

  const version = sources[0].version;
  if (options.expect && version !== options.expect) {
    failures.push(`版本号 ${version} 与期望的 ${options.expect} 不一致`);
  }

  failures.push(...checkInheritance());

  if (failures.length > 0) {
    console.error("版本校验失败：");
    for (const line of failures) {
      console.error(`  - ${line}`);
    }
    process.exit(1);
  }

  // 输出格式被 docs/RELEASE.md 引用，改动需同步文档
  console.log(`next version: ${version}`);
}

main();
