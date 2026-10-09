# RepoPrism Skill

## 这是什么

RepoPrism 是一个**只读仓库智能工具**。本 Skill 让 AI Agent 能够以结构化方式读取本地 Git 仓库的状态，用于代码审查、变更分析、PR 描述生成等任务。

**绝不执行任何 Git 写操作。** 它不 commit、不 push、不 merge、不 rebase，也不修改被观察仓库的 `.git` 目录。

## 可用命令

Agent 通过 CLI 调用：

```bash
# 输出仓库快照（HEAD、分支、标签、变更分组）
repoprism inspect <path> --json

# 输出提交历史
repoprism commits <path> --limit 100 --json

# 输出单个提交详情（含 diff）
repoprism detail <path> --sha <sha> --json
```

省略 `--json` 时输出人类可读摘要；Agent 一律加 `--json`。

## JSON Schema

所有命令返回同一个外层信封：

```json
{
  "schema_version": "1",
  "tool": "repoprism",
  "tool_version": "0.1.0",
  "data": { }
}
```

**Agent 必须先读 `schema_version`**：它为 `"1"` 时下方字段才成立。字段只增不减，不做静默改名。

### `inspect` 的 data

```json
{
  "path": "/abs/path",
  "head": {
    "branch": "main",
    "commit": "40 位完整 SHA",
    "detached": false,
    "upstream": { "name": "origin/main", "ahead": 2, "behind": 0 }
  },
  "branches": [{ "name": "main", "commit": "abc...", "is_current": true }],
  "tags": [{ "name": "v0.1.0", "commit": "abc..." }],
  "status": {
    "conflicts": [],
    "staged": [{ "path": "src/a.ts", "kind": "modified" }],
    "unstaged": []
  }
}
```

- `head.upstream` 为 `null` = **该分支没有上游**；为 `{ahead:0, behind:0}` = **有上游且已同步**。两者语义不同，不要混为一谈。
- `kind` 取值为 `added` / `modified` / `deleted` / `renamed` / `copied` / `type_changed` / `unmerged` / `unknown`。

### `commits` 的 data

数组，每项：

```json
{
  "sha": "...",
  "short_sha": "...",
  "parents": ["..."],
  "author_name": "...",
  "author_email": "...",
  "author_date": "2026-10-08T12:00:00+08:00",
  "committer_name": "...",
  "committer_email": "...",
  "committer_date": "...",
  "subject": "...",
  "body": null,
  "refs": ["main", "v0.1.0"]
}
```

`--limit` 上限 2000；`--skip` 用于分页。`refs` 含本地分支、标签与远端引用。

### `detail` 的 data

```json
{
  "info": { "...同 commits 单项..." },
  "files": [{
    "path": "src/a.ts",
    "old_path": null,
    "kind": "modified",
    "additions": 10,
    "deletions": 2,
    "binary": false
  }],
  "patch": "diff --git a/src/a.ts b/src/a.ts\n...",
  "truncated": false
}
```

- `patch` 是**未加工的 unified diff 正文**（不含提交信息头），可直接阅读或二次解析。
- `files[].binary` 为 `true` 时**只标记不读内容**；该文件不会出现在 `patch` 里。
- `truncated` 为 `true` 表示内容被截断（原始文本上限 2 MiB、结构化解析上限 5000 行）。**看到 `true` 必须告知用户「内容不完整」**，不要当成全部。
- 合并提交的 `patch` 与 `files` 都为空——这是 Git 的正常行为，不是错误。

## 典型用法

1. **生成 PR 描述**：先 `inspect` 获取变更文件，再 `commits` 对比 `main` 与当前分支，最后 `detail` 查看关键提交。
2. **代码审查准备**：`inspect` 的 staged 与 unstaged 分组直接告诉 Agent 哪些文件需要关注。
3. **理解项目结构**：`commits --limit 50` 快速浏览近期提交主题，把握项目节奏。

## 禁止事项

- 不要尝试用 RepoPrism 执行 commit、push、merge、rebase
- 不要假设 CLI 会修改仓库状态
- 如果 CLI 报错 "not a git repository"，不要重试，直接报告
