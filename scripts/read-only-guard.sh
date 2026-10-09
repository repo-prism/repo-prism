#!/usr/bin/env bash
#
# Read-only Guard — 静态扫描 repo-prism-core 的 Git 调用，禁止任何写操作命令。
#
# ## 为什么不用一行的 grep
#
# 旧实现是 `grep -rn 'Command::new("git")' ... | grep -oE 'arg\("[a-z-]+"\)'`，
# 它只检查**与 Command::new 同一行**的内容。Rust 链式调用通常把 .arg() 写在
# 后续行，于是扫描结果恒为空 —— 这是假绿灯：既抓不到跨行的 forbidden 命令，
# 也会把 `--show-toplevel` 这类合法选项误判为命令。
#
# ## 三层校验
#
# 1. **动词白名单**：把文件按语句（以 `;` 结尾）切开，取出每条语句里的双引号
#    字面量。形如 `[a-z][a-z-]*` 的「纯小写单词」只可能是 Git 子命令，
#    必须命中白名单，否则报错。这一步跨行有效，也能覆盖 `run(&["commit", ...])`
#    这种数组字面量写法。
#
# 2. **危险选项黑名单**：形如 `-x` / `--xxx` 的字面量命中危险选项即报错。
#    这一步补的是**动词白名单够不到的洞**：`git branch -D` 里 `-D` 不是单词，
#    而分支名可能含 `/`（如 `feature/x`）从而绕过单词白名单 ——
#    旧版扫描器对 `git branch -D feature/x` 完全无感。
#    选项黑名单采取保守策略：`-M` 之类在部分只读命令（`git log -M`）下是合法的
#    也会被拦下。宁可让人来加一行白名单，也不放过一次分支改名。
#
# 3. **条件动词必须带只读标志**：`stash` / `branch` / `tag` / `remote` /
#    `worktree` / `config` 这些动词本身既能读也能写（`git branch` 是列出分支，
#    `git branch x` 是创建分支）。因此要求同一语句里出现明确的只读标志
#    （如 `--list` / `--show-current` / `--get`），否则报错。
#    这一步补的是 `git config user.email X` 这类**全字面量都无懈可击**的写法：
#    键名含 `.`、值是大写，单词与选项两套规则都看不见它。
#
# 三套规则表**只存在于下方 awk 脚本里**（单一事实来源），shell 侧不重复维护。
#
# 已知边界：本扫描器是静态字面量分析，不做数据流追踪。把命令拆进运行时拼装的
# 字符串（如 `let c = ["pu","sh"].concat().join("")`）仍可绕过 —— 真正的兜底是
# 人工审查 + 本项目的"只读"承诺。扫描器的价值在于拦住无意之失，而非防恶意。
#
# 已知边界 2：**不做词法分析**，注释与代码里的 `"..."` 一视同仁。因此在注释里
# 写一个被引号包起来的纯小写词（哪怕只是举例）也会被当成 Git 子命令报违规 ——
# 2026-10-09 真被自己绊过一次。注释里要举例时，别给它加引号。
#
# 用法：
#   bash scripts/read-only-guard.sh               # 扫描 crates/repo-prism-core/src
#   bash scripts/read-only-guard.sh <dir>         # 扫描指定目录
#   bash scripts/read-only-guard.sh --self-test   # 自检：验证扫描器本身有效
#
set -uo pipefail

# awk 侧的三套规则表（用 -v 注入，便于 shell 里也能读到同一份定义）
CMD_RE='^(git|status|log|show|diff|cat-file|rev-parse|for-each-ref|branch|tag|remote|config|ls-files|ls-tree|rev-list|symbolic-ref|merge-base|describe|shortlog|blame|worktree|stash)$'
ARG_RE='^(list)$'
FLAG_RE='^-{1,2}[A-Za-z][A-Za-z-]*$'
DANGER_RE='^(-d|-D|-m|-M|-f|-u|-b|-c|--delete|--move|--force|--set|--unset|--unset-all|--replace-all|--edit|--prune|--clear|--drop|--hard|--soft|--mixed|--amend|--prune-empty)$'
COND_KEYS='stash branch tag remote worktree config'

# 扫描目录下所有 .rs 文件，输出违规项（文件:行号: token: 原因）
scan_dir() {
  local src="$1"
  find "$src" -name '*.rs' -type f -print0 \
    | xargs -0 awk -v cmd_re="$CMD_RE" \
                  -v arg_re="$ARG_RE" \
                  -v flag_re="$FLAG_RE" \
                  -v danger_re="$DANGER_RE" \
                  -v cond_keys="$COND_KEYS" '
      # 条件动词 → 可接受的只读标志（任一命中即可证明这条语句是只读的）
      function cond_re_for(key) {
        if (key == "stash")    return "^(list|show|--list)$"
        if (key == "branch")   return "^(--list|-l|--show-current|-v|--verbose|--all|-a|-r|--remotes|--contains|--merged|--no-merged|--points-at|--format=.*)$"
        if (key == "tag")      return "^(--list|-l|-n|--contains|--points-at|--merged|--no-merged|--format=.*)$"
        if (key == "remote")   return "^(-v|--verbose|show|get-url|--get-url)$"
        if (key == "worktree") return "^(list|--list)$"
        if (key == "config")   return "^(--get|--get-all|--get-regexp|--list|-l)$"
        return "^$^"
      }

      function report(tok, reason) {
        printf "%s:%d: %s: %s\n", curfile, start, tok, reason
      }

      function check(stmt,   rest, lit, n, i, verb, ok, cr) {
        n = 0
        rest = stmt
        while (match(rest, /"[^"]*"/)) {
          lit = substr(rest, RSTART + 1, RLENGTH - 2)
          rest = substr(rest, RSTART + RLENGTH)
          if (lit ~ /^[a-z][a-z-]*$/) {
            n++; toks[n] = lit; kinds[n] = "w"
          } else if (lit ~ flag_re) {
            n++; toks[n] = lit; kinds[n] = "f"
          }
          # 其余（路径、格式串、提交信息、含大写/数字/斜杠的参数）一律不参与校验
        }
        if (n == 0) return

        # 规则 1 / 2：逐 token 校验
        for (i = 1; i <= n; i++) {
          if (kinds[i] == "w") {
            if (toks[i] !~ cmd_re && toks[i] !~ arg_re) {
              report(toks[i], "白名单外的子命令/参数")
            }
          } else if (toks[i] ~ danger_re) {
            report(toks[i], "危险选项（写操作或破坏性行为）")
          }
        }

        # 规则 3：条件动词必须带只读标志
        verb = ""
        for (i = 1; i <= n; i++) {
          if (kinds[i] == "w" && toks[i] ~ cmd_re && toks[i] != "git") { verb = toks[i]; break }
        }
        if (verb != "" && index(" " cond_keys " ", " " verb " ") > 0) {
          ok = 0
          cr = cond_re_for(verb)
          for (i = 1; i <= n; i++) if (toks[i] ~ cr) { ok = 1; break }
          if (!ok) report(verb, "条件动词 " verb " 缺少明确的只读标志")
        }
      }

      # 按语句切分：Rust 里一条语句以 `;` 结尾，链式调用与数组参数都写在 `;` 之前
      FNR == 1 {
        if (buf != "") { curfile = prevfile; check(buf); buf = "" }
        prevfile = FILENAME
      }
      {
        if (buf == "") start = FNR
        buf = buf " " $0
        if ($0 ~ /;[ \t]*$/) { curfile = FILENAME; check(buf); buf = "" }
      }
      END {
        if (buf != "") { curfile = FILENAME; check(buf) }
      }
    '
}

self_test() {
  local tmp failures=0
  tmp="$(mktemp -d)"

  # 正例：期望扫描器"一声不吭"
  expect_clean() {
    local name="$1" code="$2"
    local dir="$tmp/case-$name"
    mkdir -p "$dir"
    printf '%s\n' "$code" >"$dir/git.rs"
    local out
    # 注意：${name} 必须带花括号。macOS 自带的 bash 3.2 会把紧跟 $name 的多字节
    # 字符（如「」）当作变量名的一部分，写成 `$name」` 会报 unbound variable。
    if ! out="$(scan_dir "$dir")"; then
      echo "  ✗ 正例「${name}」扫描器自身执行失败（awk 报错），不能视为通过"
      failures=$((failures + 1))
    elif [ -z "$out" ]; then
      echo "  ✓ 正例「${name}」未被误报"
    else
      echo "  ✗ 正例「${name}」被误报 -> ${out}"
      failures=$((failures + 1))
    fi
  }

  # 反例：期望扫描器报出指定 token
  expect_hit() {
    local name="$1" code="$2" token="$3"
    local dir="$tmp/case-$name"
    mkdir -p "$dir"
    printf '%s\n' "$code" >"$dir/git.rs"
    local out
    if ! out="$(scan_dir "$dir")"; then
      echo "  ✗ 反例「${name}」扫描器自身执行失败（awk 报错），不能视为通过"
      failures=$((failures + 1))
      return
    fi
    if [ -z "$out" ]; then
      echo "  ✗ 反例「${name}」漏报"
      failures=$((failures + 1))
    elif printf '%s' "$out" | grep -qF -- "$token"; then
      echo "  ✓ 反例「${name}」被检出（${token}）"
    else
      echo "  ✗ 反例「${name}」检出了，但不是预期的 ${token} -> ${out}"
      failures=$((failures + 1))
    fi
  }

  # ---- 正例 ----
  expect_clean "跨行链式与合法选项" 'let out = Command::new("git")
    .arg("-C")
    .arg(path)
    .arg("rev-parse")
    .arg("--show-toplevel")
    .output()?;'
  expect_clean "数组字面量的只读日志" 'self.run(&["log", "--all", "--date=iso-strict", "--format=%(objectname)"])?;'
  expect_clean "status 的 porcelain v2" 'self.run(&["status", "--porcelain=v2", "--branch", "-z"])?;'

  # ---- 正例：条件动词 + 明确的只读标志 ----
  expect_clean "stash list" 'self.run(&["stash", "list"])?;'
  expect_clean "branch --list" 'self.run(&["branch", "--list"])?;'
  expect_clean "branch --show-current" 'self.run(&["branch", "--show-current"])?;'
  expect_clean "config --get" 'self.run(&["config", "--get", "user.email"])?;'
  expect_clean "config --list" 'self.run(&["config", "--list"])?;'
  expect_clean "tag --list" 'self.run(&["tag", "--list"])?;'
  expect_clean "tag -l" 'self.run(&["tag", "-l"])?;'

  # ---- 反例：写操作 ----
  expect_hit "跨行 .arg 写操作" 'let out = Command::new("git")
    .arg("-C")
    .arg(path)
    .arg("push")
    .output()?;' "push"
  expect_hit "数组字面量写操作" 'self.run(&["commit", "--amend", "--no-edit"]);' "commit"
  expect_hit "破坏性命令" 'Command::new("git").arg("-C").arg(p).arg("reset").arg("--hard");' "reset"

  # ---- 反例：旧版扫描器全部漏报的 5 种写法 ----
  expect_hit "stash 的写子命令" 'self.run(&["stash", "pop"]);' "pop"
  expect_hit "分支删除（分支名含斜杠，绕过单词白名单）" 'self.run(&["branch", "-D", "feature/x"]);' "-D"
  expect_hit "配置写入（键含点、值大写，单词与选项规则都看不见）" 'self.run(&["config", "user.email", "ME@EXAMPLE.COM"]);' "config"
  expect_hit "工作树删除" 'self.run(&["worktree", "remove", "/tmp/wt"]);' "remove"
  expect_hit "标签删除" 'self.run(&["tag", "-d", "v1"]);' "-d"

  rm -rf "$tmp"

  if [ "$failures" -ne 0 ]; then
    echo "Read-only guard 自检失败（$failures 项）"
    return 1
  fi
  echo "Read-only guard 自检通过"
}

main() {
  case "${1:-}" in
    --self-test)
      self_test
      ;;
    *)
      local src="${1:-crates/repo-prism-core/src}"
      if [ ! -d "$src" ]; then
        echo "core crate not found at ${src}, skipping"
        exit 0
      fi
      local violations
      # 扫描器自身报错（awk 语法/运行时错误）必须当作失败退出。
      # 否则 awk 崩掉 -> 输出为空 -> 打印 "passed"，正是本脚本要消灭的假绿灯。
      if ! violations="$(scan_dir "$src")"; then
        echo "ERROR: read-only guard 扫描器自身执行失败，扫描结果不可信，按失败处理"
        exit 2
      fi
      if [ -n "$violations" ]; then
        echo "ERROR: 检测到白名单外的 Git 调用："
        echo "$violations"
        exit 1
      fi
      echo "Read-only guard passed"
      ;;
  esac
}

main "$@"
