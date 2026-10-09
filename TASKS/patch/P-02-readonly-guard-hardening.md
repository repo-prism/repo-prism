# P-02: 只读白名单加固到参数级

**状态**：已完成（2026-10-09）
**卡号说明**：本卡不在 001–020 的产品血统内，属工程补丁，故编入 patch 序列（见 [README](README.md)）。

## 目标

把 `scripts/read-only-guard.sh` 的白名单从「只校验第一个 token」提升为
「动词 + 危险选项 + 条件动词读标志」三层校验，堵住「合法动词 + 写子命令/写选项」的绕过路径。

## 上下文

扫描器**机制**层面的问题此前已修好（跨行 `.arg()`、数组字面量、选项误判）。残留问题是**粒度**：

`ALLOWED` 白名单只校验首个 token，而 `AGENTS.md` 明确禁止 `stash pop`、`branch -d`：

| 白名单放行 | 实际可写成 |
|-----------|-----------|
| `stash` | `git stash pop` / `drop` / `clear` / 裸 `git stash` |
| `branch` | `git branch -D feature/x`（分支名含 `/` → 单词白名单看不见） |
| `tag` | `git tag -d v1` |
| `config` | `git config user.email X`（键含点、值大写 → 单词与选项两层都看不见） |
| `remote` | `git remote add` |
| `worktree` | `git worktree remove` |

## 实现：三层校验

| 层 | 规则 | 补的洞 |
|----|------|--------|
| 1 | 语句内 `[a-z][a-z-]*` 字面量必须是已知只读子命令（或 `list`） | `pop` / `add` / `remove` 等写子命令 |
| 2 | `-x` / `--xxx` 字面量不得命中危险选项表 | `-D` / `-d` / `-m` / `-f` / `--delete` / `--force` / `--unset` … |
| 3 | 条件动词必须带明确读标志 | `git config user.email X` 这类全字面量都无懈可击的写法 |

实现要点：

- 先按 `;` 把文件切成语句（Rust 链式调用与数组参数都写在 `;` 之前），
  再在语句内按出现顺序抽取字面量并分类：单词 / 选项 / 其余（路径、格式串等一律不参与校验）
- 规则表**只存在于 awk 脚本里**（用 `-v` 注入），shell 侧不重复维护第二份

### 自认的代价（写进代码注释，避免被当成 bug）

危险选项黑名单采取保守策略：`-M` 在 `git log -M`（重命名检测）下是合法的，也会被拦下。
**宁可让人来加一行白名单，也不放过一次分支改名。**

### 顺带修掉的一个真实假绿灯

验证时发现：awk 语法报错 → 输出为空 → `main` 判定「无违规」→ 打印 `Read-only guard passed`。
**扫描器崩了却报通过**，正是本卡要消灭的那类问题。现在扫描器自身失败以**退出码 2** 失败。

（另记一个环境坑：macOS 自带 bash 3.2 会把紧跟 `$name` 的多字节字符当作变量名的一部分，
`$name」` 会报 unbound variable。脚本内已改为 `${name}` 并留了注释。）

## 验收标准

- [x] `--self-test` 新增 5 个反例并全部检出：
  - [x] `git stash pop`
  - [x] `git branch -D feature/x`
  - [x] `git config user.email ME@EXAMPLE.COM`
  - [x] `git worktree remove /tmp/wt`
  - [x] `git tag -d v1`
- [x] `--self-test` 正例仍不误报，新增：
  - [x] `git stash list`
  - [x] `git branch --list` / `git branch --show-current`
  - [x] `git config --get user.email` / `git config --list`
  - [x] `git tag --list` / `git tag -l`
- [x] 对当前 `crates/repo-prism-core/src` 仍为 passed（**且 TASK-007 新增的
      `show --name-status -z --find-renames --no-textconv` 等调用全部放行**）
- [x] `.github/workflows/ci.yml` 的 `Self-test the guard` 步骤覆盖新反例（自检自动纳入）
- [x] 扫描器自身失败不再被当作通过（退出码 2）

自检结果：**18/18 通过**（10 正例 + 8 反例）。

## 禁止事项

- [x] 未放宽为 token 级白名单
- [x] 未引入 Python/Node 依赖（纯 bash + awk）
