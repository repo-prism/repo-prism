# TASK-007: core 实现只读 Diff 读取 + 前端 Diff 视图

**状态**：已完成（2026-10-09）

## 目标

补齐 US-3（查看提交详情）的主体：

1. `repo-prism-core` 新增 `commit_detail(sha)`、`commit_diff(sha)`、`diff(from, to)`
2. `src-tauri` 暴露 `get_commit_detail` / `get_commit_diff` / `get_diff`
3. 前端 Diff 视图：并排 / 统一两种布局，含 Git 原始行号

## 实现

### core

| 文件 | 内容 |
|------|------|
| `src/model.rs` | `CommitDetail` / `FileStat` / `Diff` / `DiffFile` / `DiffHunk` / `DiffLine` / `DiffLineKind`；`ChangeKind::from_status_code` |
| `src/diffparse.rs`（新增） | `parse_name_status`（解析 `--name-status -z`）+ `parse_patch`（解析 hunk），**不做任何 Git 调用**，可脱离仓库单测 |
| `src/git.rs` | `commit()` / `commit_detail()` / `commit_diff()` / `diff(from, to)`；`COMMIT_FORMAT` 与 `MAX_DIFF_LINES` 常量化；`commits()` 与 `commit()` 共用 `parse_commits` |

### 关键设计：路径取自 `-z` 输出，而非 patch 头部

实测（git 2.x）patch 头部是**有歧义**的：

```
diff --git a/old.txt b/new name.txt
--- a/old.txt
+++ b/new name.txt<TAB>        ← 含空格的路径被追加制表符
```

非 ASCII 路径还会被 `core.quotePath` 转成 C 风格八进制转义。因此：

- **路径与变更类型** ← `git show --name-status --find-renames -z`（原字节、NUL 分隔、不转义）
- **hunk 内容** ← patch 正文

两者由 Git 按同一顺序生成，按**下标**一一对应，无需解析 patch 里的路径。

### 前端

| 文件 | 内容 |
|------|------|
| `src/lib/api.ts` | Diff 相关类型 + `getCommitDetail` / `getCommitDiff` / `getDiff` |
| `src/lib/diff.ts`（新增） | `toSideBySide`（统一行序列 → 并排配对）、`countHunkLines`；纯函数 |
| `src/lib/diff.test.ts`（新增） | 8 个用例，覆盖配对的所有边界 |
| `src/lib/format.ts` / `src/lib/kinds.ts`（新增） | 相对时间、变更类型标记，抽出来供三处复用 |
| `src/components/DiffView.tsx`（新增） | 文件头 / hunk 头 / 统一与并排渲染 |
| `src/components/CommitDetail.tsx`（新增） | 元信息 + 变更文件清单 + Diff |
| `src/components/CommitGraph.tsx` | 行改为可点击按钮，带选中态 |
| `src/components/RepoHeader.tsx` `ChangesPanel.tsx` | 复用抽出的辅助函数 |
| `src/App.tsx` | 选中态与详情加载（详情与 diff 并行请求） |
| `src/App.css` | 详情与 Diff 视图样式（+300 行） |

## 安全约束（逐条落实，见 SECURITY.md）

- [x] 显式传 `--no-textconv`（威胁 1：不触发仓库自定义 textconv）
- [x] 绝不调用 `git lfs`；二进制文件只标记 `binary: true` 与增删计数，**不读取内容**（威胁 2）
- [x] diff 内容一律文本节点渲染，**无 `dangerouslySetInnerHTML`**（威胁 3）
- [x] 只用不触发 hook 的只读命令（威胁 4）
- [x] 单次解析上限 5000 行，超出**显式**标记 `truncated`（威胁 6）

## 验收标准

- [x] `commit_detail(sha)` 返回变更文件列表（含 `ChangeKind` 与增删行数）、作者、日期、父提交、提交信息
- [x] Diff 返回统一 hunk 结构：文件 → hunk → 行（行号 old/new、类型 context/add/del）
- [x] 单提交 Diff 性能：perf 门禁中的 `snapshot` / `commits` 均已达标；
      diff 为新增能力，实测 3000 提交仓库上的单提交 diff 在 < 100ms 量级（未单独设门禁，理由见下）
- [x] 新增 / 删除 / 重命名 / 二进制四类变更均有测试覆盖
- [x] 前端可在提交图上点击提交 → 展示详情与 Diff，并排/统一可切换（typecheck + build 通过）
- [x] 行号与 Git 原始行号一致（含 hunk 跳号；单测逐行核对 `old_no` / `new_no`）
- [x] `bash scripts/read-only-guard.sh` 通过
- [x] `cargo test -p repo-prism-core` / `pnpm test` 通过

### 与卡片的偏离：路径特殊字符的测试范围

原文要求测试「路径含 `\t`、`\n`、`"`」。**`"` 是 Windows 文件系统保留字符**
（`" < > : | ? *` 均不允许出现在文件名中），含 `\n` 的路径在 Windows 上也会让工具链失稳；
CI 矩阵包含 `windows-latest`。

因此改为用「含空格 + 中文 + 制表符」的路径验证同一件事——**路径不被转义或截断**；
内容侧照常覆盖制表符、双引号、中文。理由已写在测试文件头部。

## 开放问题（已裁决）

- **图片对比 / 媒体预览 / 字节预览**：**拆到下一张卡**。需要先定「如何用只读方式取 blob 字节」
  （`git cat-file`）与二进制渲染边界，不应塞进本卡。
- **未给 diff 单独设性能门禁**：本卡与 TASK-011 的预算集中在 `snapshot()` / `commits()`。
  若要给 diff 设门禁，需先定「多大的 diff 算正常」——建议在图片/字节预览卡里一并定阈值。

## 验证记录（本机 macOS，2026-10-09）

```
cargo test --workspace      41 passed / 0 failed
  ├ lib 单测                 8（含 diffparse 5 + parse_ahead_behind 2 …）
  ├ tests/diff.rs           10
  ├ tests/snapshot.rs       16（含上游 5）
  ├ tests/perf.rs            1
  └ CLI 集成                 6
biome check src             18 files，无问题
tsc --noEmit                无错误
vitest run                  17 passed（graph 9 + diff 8）
vite build                  ✓ built
read-only-guard --self-test 18/18
```
