# SECURITY.md — RepoPrism 只读安全模型

## 核心承诺

**RepoPrism 绝不修改被观察仓库的任何状态。**

这不是一个功能限制，而是一个**安全边界**。它让 RepoPrism 可以安全地嵌入任何工作流，与用户的 Git 操作并行运行而不产生冲突。

## 威胁模型与应对

### 威胁 1：恶意仓库通过 content filter 执行任意代码
**场景**：仓库的 `.gitattributes` 定义了 external filter，Git 在读取时触发执行。
**应对**：RepoPrism 不运行任何 external filter 或转换器。读取时使用 `git cat-file`、`git show` 等不触发 filter 的底层命令。若必须读取经过 filter 的内容，展示原始字节并说明限制。

### 威胁 2：恶意仓库的 LFS 指针触发网络请求
**场景**：`git lfs` 在读取时自动下载大文件，可能泄露信息或消耗带宽。
**应对**：只读取标准 LFS 指针文件（几行文本），展示 object ID 和 size，**绝不调用 `git lfs` 命令**。

### 威胁 3：恶意 diff 注入 UI
**场景**：diff 内容包含 HTML/JS，在 UI 中渲染时执行。
**应对**：diff 渲染使用纯文本 + 语法高亮，**不使用 `dangerouslySetInnerHTML`**，不执行任何来自仓库的内容。

### 威胁 4：恶意 hook 触发
**场景**：Git 操作触发 `.git/hooks/` 中的脚本。
**应对**：RepoPrism 只使用不触发 hook 的只读命令（`status`、`log`、`show`、`diff`、`cat-file`、`rev-parse`、`for-each-ref`）。**绝不使用 `git checkout`、`git commit`、`git merge` 等会触发 hook 的命令**。

### 威胁 5：AI 生成的代码意外引入写操作
**场景**：AI Agent 实现功能时调用了 `git commit`。
**应对**：CI 中强制静态扫描，见下节。

## CI 只读扫描

在 `.github/workflows/ci.yml` 中，对 `crates/repo-prism-core` 执行：

```bash
# 提取所有 git 子命令调用
grep -rn 'Command::new("git")' crates/repo-prism-core/src/ \
  | grep -oE 'arg\("[a-z-]+"\)' \
  | sort -u > /tmp/git-commands.txt

# 白名单：只允许这些只读子命令
ALLOWED="arg(\"status\")
arg(\"log\")
arg(\"show\")
arg(\"diff\")
arg(\"cat-file\")
arg(\"rev-parse\")
arg(\"for-each-ref\")
arg(\"branch\")
arg(\"tag\")
arg(\"remote\")
arg(\"config\")
arg(\"ls-files\")
arg(\"ls-tree\")
arg(\"rev-list\")
arg(\"symbolic-ref\")
arg(\"merge-base\")
arg(\"describe\")
arg(\"shortlog\")
arg(\"blame\")
arg(\"worktree\")
arg(\"stash\")"

# 检查是否有白名单外的命令
while read cmd; do
  if ! echo "$ALLOWED" | grep -qF "$cmd"; then
    echo "ERROR: 检测到非白名单 Git 命令: $cmd"
    exit 1
  fi
done < /tmp/git-commands.txt