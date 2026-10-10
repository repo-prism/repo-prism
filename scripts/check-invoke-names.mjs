#!/usr/bin/env node
// 前端 `invoke("...")` 的字符串必须能在后端 `generate_handler!` 里找到。
//
// 为什么需要它：命令名是**字符串**，两端没有编译期的联系。
// 改了一边忘了另一边，构建全绿、lint 全绿、测试全绿，只有用户点了那个按钮
// 才会在运行时炸 —— 而 UI 接线层正好是全项目唯一没有自动化测试的地方。
//
// 它抓不到什么：参数名对不对（`{ filePath }` ↔ `file_path`）、参数类型对不对。
// 那是形状层面的事，得靠真跑。本脚本只守「名字存在」这一层。

import { readFileSync } from "node:fs";

const API = "src/lib/api.ts";
const BACKEND = "src-tauri/src/lib.rs";

function fail(reasons) {
  console.error("命令名校验失败：");
  for (const r of reasons) console.error(`  - ${r}`);
  process.exit(1);
}

const apiSrc = readFileSync(API, "utf8");
const backendSrc = readFileSync(BACKEND, "utf8");

// 前端：invoke<...>("name", ...)
const invoked = new Set(
  [...apiSrc.matchAll(/invoke\s*(?:<[^>]*>)?\s*\(\s*"([a-z0-9_]+)"/g)].map((m) => m[1]),
);

// 后端：tauri::generate_handler![ ... ] 里列出的函数名
const block = backendSrc.split("tauri::generate_handler![")[1];
if (!block) fail([`在 ${BACKEND} 里找不到 generate_handler!`]);
const registered = new Set(
  block
    .split("]")[0]
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean),
);

if (invoked.size === 0) fail([`在 ${API} 里没找到任何 invoke 调用 —— 是不是正则或文件变了？`]);
if (registered.size === 0) fail([`generate_handler! 里是空的`]);

const missing = [...invoked].filter((name) => !registered.has(name)).sort();
if (missing.length > 0) {
  fail(missing.map((n) => `前端调了 ${n}，但后端没有注册它（用户点下去会运行时报错）`));
}

const unused = [...registered].filter((name) => !invoked.has(name)).sort();
if (unused.length > 0) {
  // 不算失败：后端可以先注册、前端后接。但值得看见。
  console.warn(`提示：后端注册了但前端还没调用：${unused.join(", ")}`);
}

console.log(`命令名一致：前端 ${invoked.size} 个 / 后端 ${registered.size} 个，全部对得上`);
