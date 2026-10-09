#!/usr/bin/env node
// 版本一致性校验 —— 发布硬化的一部分。
//
// 为什么需要它：版本号在本项目里有**三处**独立声明，谁都能单独改。
// 只改一处就会出一个「界面显示 0.2.0、安装包元数据写 0.1.0」的版本，
// 而且直到用户装完才发现。这个脚本把它们绑成一个断言：
//
//   1. package.json                → version
//   2. Cargo.toml（workspace）     → [workspace.package] version
//   3. src-tauri/tauri.conf.json   → version
//
// 另外校验两件同样容易漏、而且会真的炸掉发布的事：
//
//   4. **每个 workspace 成员都必须继承版本**（`version.workspace = true`）。
//      成员清单直接从根 Cargo.toml 的 `[workspace] members` 里读，不再依赖
//      「crates/ 下面一层就是 crate」这种目录假设 —— v0.2.0 正是栽在这个假设上：
//      src-tauri 自己**就是**那个 crate，不是装 crate 的目录，于是它硬编码的
//      `version = "0.1.0"` 一路放行，直到发版前人工核对才发现。
//   5. **Cargo.lock 里每个成员的版本必须与当前版本号一致**。
//      release.yml 用 `--locked` 构建 CLI / MCP，锁文件过期会在发布那一刻直接失败；
//      而 CI 的 clippy / test 不带 `--locked`，反手就把锁文件改掉、当场变绿 ——
//      这个错只有打 tag 时才炸，所以必须在这里拦住。
//
// 用法：
//   node scripts/check-versions.mjs                      # 只校验一致性
//   node scripts/check-versions.mjs --expect 0.2.0       # 还要求等于指定版本
//   node scripts/check-versions.mjs --expect-ref v0.2.0  # 用 tag 校验（自动去 v）

import { existsSync, readFileSync } from "node:fs";
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

/** 读 TOML 里某个段（`[section]`）下 `key = "value"` 的值，读不到返回 null。 */
function sectionValue(text, section, key) {
  let inside = false;
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("[")) {
      inside = line === section;
      continue;
    }
    if (!inside) continue;
    const match = new RegExp(`^${key}\\s*=\\s*"([^"]+)"`).exec(line);
    if (match) return match[1];
  }
  return null;
}

/** 根 Cargo.toml 的 `[workspace] members` 列出的成员目录。 */
function workspaceMembers() {
  const match = /\[workspace\][\s\S]*?members\s*=\s*\[([\s\S]*?)\]/.exec(read("Cargo.toml"));
  if (!match) {
    throw new Error("Cargo.toml 里找不到 [workspace] members");
  }
  return [...match[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

/** 成员在 `[package]` 段声明的包名。 */
function packageName(member) {
  return sectionValue(read(`${member}/Cargo.toml`), "[package]", "name");
}

/** 4. 成员必须是继承版本，不能硬编码。 */
function checkInheritance() {
  const problems = [];
  for (const member of workspaceMembers()) {
    const manifest = `${member}/Cargo.toml`;
    // `version = "…"` 命中即为硬编码；`version.workspace = true` 不会命中
    // （`version` 后面跟的是 `.`，不是 `=`）。
    if (/^version\s*=/m.test(read(manifest))) {
      problems.push(`${manifest} 硬编码了 version，应改成 version.workspace = true`);
    }
  }
  return problems;
}

/** 5. Cargo.lock 里的成员版本必须与当前版本号一致。 */
function checkLockfile(version) {
  const problems = [];
  const lock = read("Cargo.lock");
  for (const member of workspaceMembers()) {
    const name = packageName(member);
    if (!name) {
      problems.push(`${member}/Cargo.toml 的 [package] 段里没有 name`);
      continue;
    }
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const match = new RegExp(
      `\\[\\[package\\]\\]\\nname = "${escaped}"\\nversion = "([^"]+)"`,
    ).exec(lock);
    if (!match) {
      problems.push(`Cargo.lock 里找不到 workspace 成员 ${name}`);
    } else if (match[1] !== version) {
      problems.push(
        `Cargo.lock 里 ${name} 是 ${match[1]}，与 ${version} 不一致 —— 改版本号时` +
          "必须把 Cargo.lock 一起提交，否则 release 的 --locked 构建会失败",
      );
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
    { path: "package.json", version: JSON.parse(read("package.json")).version },
    {
      path: "Cargo.toml [workspace.package]",
      version: sectionValue(read("Cargo.toml"), "[workspace.package]", "version"),
    },
    {
      path: "src-tauri/tauri.conf.json",
      version: JSON.parse(read("src-tauri/tauri.conf.json")).version,
    },
  ];

  const failures = [];

  const unreadable = sources.filter((source) => !source.version);
  if (unreadable.length > 0) {
    failures.push(`读不到版本号：${unreadable.map((source) => source.path).join("、")}`);
  } else if (new Set(sources.map((source) => source.version)).size !== 1) {
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
  failures.push(...checkLockfile(version));

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
