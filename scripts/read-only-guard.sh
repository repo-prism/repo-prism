#!/usr/bin/env bash
#
# Read-only Guard — 静态扫描 repo-prism-core 的 Git 调用，禁止任何写操作命令。
#
# 为什么不用一行的 grep：
#   旧实现是 `grep -rn 'Command::new("git")' ... | grep -oE 'arg\("[a-z-]+"\)'`，
#   它只检查**与 Command::new 同一行**的内容。Rust 链式调用通常把 .arg() 写在
#   后续行，于是扫描结果恒为空 —— 这是假绿灯：既抓不到跨行的 forbidden 命令，
#   也会把 `--show-toplevel` 这类合法选项误判为命令。
#
# 本脚本的做法：
#   1. 扫描 core/src 下**全部** .rs 文件的字符串字面量（不受换行位置影响，
#      也能覆盖 `run(&["commit", ...])` 这种数组字面量写法）；
#   2. 只把「纯小写字母 + 连字符」的 token 当作子命令候选，从而跳过：
#      选项（`--show-toplevel`、`-C`）、含大写/数字/特殊字符的参数与格式串
#      （`HEAD`、`%(objectname)`、`refs/heads/`）；
#   3. 候选必须命中白名单，否则报错并给出 文件:行号。
#
# 用法：
#   bash scripts/read-only-guard.sh               # 扫描 crates/repo-prism-core/src
#   bash scripts/read-only-guard.sh <dir>         # 扫描指定目录
#   bash scripts/read-only-guard.sh --self-test   # 自检：验证扫描器本身有效
#
set -uo pipefail

# 允许出现的只读子命令（`git` 是可执行文件名，出现在 Command::new("git") 中，一并放行）
ALLOWED='git|status|log|show|diff|cat-file|rev-parse|for-each-ref|branch|tag|remote|config|ls-files|ls-tree|rev-list|symbolic-ref|merge-base|describe|shortlog|blame|worktree|stash'

# 扫描目录下所有 .rs 文件，输出不在白名单内的子命令候选（文件:行号: token）
scan_dir() {
  local src="$1"
  find "$src" -name '*.rs' -type f -print0 \
    | xargs -0 awk -v allowed="$ALLOWED" '
        {
          line = $0
          while (match(line, /"[a-z][a-z-]*"/)) {
            tok = substr(line, RSTART + 1, RLENGTH - 2)
            if (tok !~ "^(" allowed ")$") {
              printf "%s:%d: %s\n", FILENAME, FNR, tok
            }
            line = substr(line, RSTART + RLENGTH)
          }
        }
      '
}

self_test() {
  local tmp failures=0
  tmp="$(mktemp -d)"

  # 正例：合法的只读调用（跨行链式 + 合法选项），应通过
  mkdir -p "$tmp/ok"
  cat >"$tmp/ok/git.rs" <<'EOF'
let out = Command::new("git")
    .arg("-C")
    .arg(path)
    .arg("rev-parse")
    .arg("--show-toplevel")
    .output()?;
EOF
  if out="$(scan_dir "$tmp/ok")" && [ -z "$out" ]; then
    echo "  ✓ 正例：合法只读调用未被误报"
  else
    echo "  ✗ 正例失败：合法调用被误报 -> $out"
    failures=$((failures + 1))
  fi

  # 反例 1：写操作写在链式调用的下一行（旧版扫描器漏报的场景）
  mkdir -p "$tmp/bad-arg"
  cat >"$tmp/bad-arg/git.rs" <<'EOF'
let out = Command::new("git")
    .arg("-C")
    .arg(path)
    .arg("push")
    .output()?;
EOF
  if out="$(scan_dir "$tmp/bad-arg")" && [ -n "$out" ]; then
    echo "  ✓ 反例 1：跨行 .arg(\"push\") 被检出"
  else
    echo "  ✗ 反例 1 失败：跨行写操作未被检出"
    failures=$((failures + 1))
  fi

  # 反例 2：写操作藏在数组字面量里（旧版扫描器同样漏报）
  mkdir -p "$tmp/bad-array"
  cat >"$tmp/bad-array/git.rs" <<'EOF'
self.run(&["commit", "--amend", "--no-edit"]);
EOF
  if out="$(scan_dir "$tmp/bad-array")" && [ -n "$out" ]; then
    echo "  ✓ 反例 2：数组字面量中的 commit 被检出"
  else
    echo "  ✗ 反例 2 失败：数组字面量中的写操作未被检出"
    failures=$((failures + 1))
  fi

  # 反例 3：明确的破坏性命令
  mkdir -p "$tmp/bad-reset"
  cat >"$tmp/bad-reset/git.rs" <<'EOF'
Command::new("git").arg("-C").arg(p).arg("reset").arg("--hard");
EOF
  if out="$(scan_dir "$tmp/bad-reset")" && [ -n "$out" ]; then
    echo "  ✓ 反例 3：reset 被检出"
  else
    echo "  ✗ 反例 3 失败：reset 未被检出"
    failures=$((failures + 1))
  fi

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
        echo "core crate not found at $src, skipping"
        exit 0
      fi
      local violations
      violations="$(scan_dir "$src")"
      if [ -n "$violations" ]; then
        echo "ERROR: 检测到白名单外的 Git 子命令："
        echo "$violations"
        exit 1
      fi
      echo "Read-only guard passed"
      ;;
  esac
}

main "$@"
