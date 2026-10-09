# P-01: 补全上游跟踪 ahead/behind

**状态**：已完成（2026-10-09）
**卡号说明**：本卡不在 001–020 的产品血统内，属工程补丁，故编入 patch 序列（见 [README](README.md)）。

## 目标

让 `RepoSnapshot.head.upstream` 返回真实数据，补齐 US-1「本地分支列表、上游跟踪计数」。

## 上下文

**数据已经在手上，只是被丢掉了。**

`git.rs` 已在执行 `git status --porcelain=v2 --branch -z`，该输出本身就包含：

```
# branch.upstream origin/main
# branch.ab +2 -0
```

但原先的分支解析第一句是 `entry.starts_with("# ") → continue`，把 `# branch.*` 整行跳过；
`head()` 又硬编码 `upstream: None`。模型侧的 `UpstreamInfo` 已存在且前端已声明其类型，
**不需要改模型，只需要接上解析**。

实测（`od -c`）确认：`-z` 下每个头部行是**独立的 NUL 结尾记录**，不是多行合并成一条，
因此不需要按换行切分——路径中含换行符也不会破坏解析。

## 约束

- 无上游分支时 `upstream` 必须为 `None`，不得退化为 `ahead=0, behind=0`
- `+` / `-` 前缀需正确剥离，`ahead` / `behind` 为 `u32`
- 不额外起 `git rev-list --count` 进程——那份数据已在上游输出里

## 实际改动

| 文件 | 改动 |
|------|------|
| `crates/repo-prism-core/src/git.rs` | `status()` 返回 `(StatusInfo, Option<UpstreamInfo>)`；`head()` 接收 upstream；新增 `parse_ahead_behind` 与其单元测试 |
| `crates/repo-prism-core/tests/common/mod.rs` | 新增 `TempRepo::new_bare` / `add_upstream`，抽取 `temp_path` |
| `crates/repo-prism-core/tests/snapshot.rs` | 新增 5 个上游场景用例 |
| `src/components/RepoHeader.tsx` | 展示上游名与 `↑n ↓m`；无上游显式显示「无上游分支」 |
| `src/App.css` | `.repo-upstream` 系列样式 |

**一个刻意的区分**：`upstream === null`（没有上游）与 `ahead=0, behind=0`（与上游同步）
在 UI 上渲染成不同文案。把两者都画成 `↑0 ↓0` 会让用户以为「已同步」。

## 验收标准

- [x] 有上游且同步：`upstream.name == "origin/main"`，`ahead == 0`，`behind == 0`
- [x] 本地领先上游 2 个提交：`ahead == 2`
- [x] 本地落后上游 3 个提交：`behind == 3`
- [x] 无上游分支：`upstream == None`
- [x] `snapshot()` 不因本卡变慢（仍是同一次 `status` 调用，未新增子进程；
      perf 门禁实测 208ms / 预算 500ms）
- [x] 前端 `RepoHeader` 展示上游与 `↑n ↓m` 计数
- [x] `cargo test -p repo-prism-core` 通过（新增 5 个上游用例 + 2 个 `parse_ahead_behind` 单测）

## 禁止事项

- [x] 未使用 `git fetch` / `git pull`（测试夹具用本地 bare 仓库，全程离线）
- [x] 未在 core 之外解析 Git 输出
