# SECURITY.md — RepoPrism 只读安全模型

## 核心承诺

**RepoPrism 绝不修改被观察仓库的任何状态。**

这不是一个功能限制，而是一个**安全边界**。它让 RepoPrism 可以安全地嵌入任何工作流，与用户的 Git 操作并行运行而不产生冲突。

## 威胁模型与应对

### 威胁 1：恶意仓库通过 content filter 执行任意代码
**场景**：仓库的 `.gitattributes` 定义了 external textconv filter，Git 在生成 diff 时触发执行。
**应对**：读取 diff 时**显式传 `--no-textconv`**。读取 blob 使用 `git cat-file`、`git show` 等不触发 filter 的底层命令。
若必须读取经过 filter 的内容，展示原始字节并说明限制。
CI 扫描器把 `--textconv` / `--filters` 列入**危险选项黑名单**：真传了就会红，
而不是只靠人工记着别传。

### 威胁 2：恶意仓库的 LFS 指针触发网络请求
**场景**：`git lfs` 在读取时自动下载大文件，可能泄露信息或消耗带宽。
**应对**：只读取标准 LFS 指针文件（几行文本），展示 object ID 和 size，**绝不调用 `git lfs` 命令**。
二进制文件在 diff 中只标记 `binary: true` 与增删计数，**不读取内容**。

### 威胁 3：恶意 diff 注入 UI
**场景**：diff 内容包含 HTML/JS，在 UI 中渲染时执行。
**应对**：所有 diff 内容都以**文本节点**写入 DOM（`code` / `span` 的 children），
**不使用 `dangerouslySetInnerHTML`**，不执行任何来自仓库的内容。

### 威胁 4：恶意 hook 触发
**场景**：Git 操作触发 `.git/hooks/` 中的脚本。
**应对**：RepoPrism 只使用不触发 hook 的只读命令（`status`、`log`、`show`、`diff`、`cat-file`、`rev-parse`、`for-each-ref`、`symbolic-ref`）。**绝不使用 `git checkout`、`git commit`、`git merge` 等会触发 hook 的命令**。

### 威胁 5：AI 生成的代码意外引入写操作
**场景**：AI Agent 实现功能时调用了 `git commit`、`git stash pop` 或 `git branch -D`。
**应对**：CI 中强制静态扫描，见下节。

### 威胁 6：病理仓库导致资源耗尽
**场景**：一次 diff 体积达到数百 MB，或单文件变更行数极大，拖垮内存与 UI。
**应对**：单次 diff 解析有 **5000 行上限**，原始 patch 文本另有 **2 MiB 上限**；
超出即停止解析，并把受影响文件与整体都标记 `truncated`。截断是**显式**的，
用户能看到「已截断」，而不是拿到一份静默缺失的 diff。
按字节截断必须回退到字符边界，否则会在多字节字符中间切开。

### 威胁 7：代码被发往外部服务

**场景**：可选的本地 AI 摘要层需要发 HTTP 请求。若校验写成「比前缀」，
`http://localhost.evil.com`（前缀命中）与 `http://localhost@evil.com`
（URL 语义里 `localhost` 是 userinfo，真实 host 是 `evil.com`）都会被放行 ——
用户的代码就被送到了外部地址。

**应对**（四层，缺一不可）：

1. **真解析而非比前缀**：拆出 authority → 拒绝含 `@` → 拆出 host 与 port →
   host 精确比对 `localhost` / `127.0.0.1` / `::1`。前缀判定与 `[::1]evil.com`
   这类写法一起被 `rejects_hosts_that_only_look_local` 的 8 条输入钉住。
2. **收窄 scheme 与依赖**：只允许 `http`（回环地址上 https 无实际用途），
   `ureq` 关闭默认特性不带 TLS —— 依赖树里没有 `rustls` / `ring` / `webpki`，
   也就不存在「哪天有人顺手打开一个云端 https endpoint」的机会。
3. **默认关闭 + 只送最小内容**：`enabled` 为 false 时摘要器根本不会被构造；
   送出的只有文件的**相对路径与规则结果**，不含仓库绝对路径、remote URL、
   diff 正文与文件内容。
4. **不遵循环境代理**：`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 这类环境变量
   **会改写连接目标**——若被遵循，endpoint 上写着的 `127.0.0.1` 就不再是真正要连的
   地址，第 1 层校验等于白做，而字节会先送到代理（企业网、CI、容器里代理是常态）。
   因此请求显式使用构造出来的、**不读代理环境变量**的 agent
   （`summarizer::local_agent`，**刻意不调用** `AgentBuilder::proxy_from_env()`），
   并由 P-08 的 `tests/summarizer_egress.rs` 用一个指向**死端口**的代理把这条性质钉住。

**必须知道的边界**：CI 的只读扫描器**看不见 HTTP 调用**（它是 Git 动词白名单）。
本条约束**只由测试兜底**，不是静态扫描兜底。三层：

```
cargo test -p repo-prism-core --lib summarizer      # 校验逻辑（纯函数）
cargo test -p repo-prism-core --test summarizer_http     # P-07：回环 stub 真实往返
cargo test -p repo-prism-core --test summarizer_egress   # P-08：代理不得改道回环请求
cargo test -p repo-prism-core --test summarizer_contract # P-08：与官方 API 文档的字段契约
```

改动 `summarizer.rs` 的校验或请求构造时，这四条都要跑。

> P-07 之前，第 3 层（「只送最小内容」）**只有代码审查，没有测试** ——
> 那两处 `ureq` 调用从未被执行过。第 3 层现在由
> `the_bytes_that_leave_the_machine_carry_relative_paths_only` 直接断言请求体。
>
> P-08 之前，第 4 层（代理改道）**连文档都没写**，只是一条隐含假设：
> 请求能被送达是因为 endpoint 写着回环地址，而没有人问过「这个地址是不是真的被用到了」。
> 现在它既写进了代码（`local_agent`），也被测试钉住。

`set_ai_settings` 的顺序也是这条边界的一部分：**先校验、再落盘、再入内存**。
先存后校验等于允许用户绕开唯一那道闸门。

---

## CI 只读扫描

扫描器是 **`scripts/read-only-guard.sh`** —— 它是白名单的**唯一事实来源**，
本文件不再重复维护一份名单（重复过的名单必然与实现漂移，这正是之前发生过的事）。

在 `.github/workflows/ci.yml` 中：

```bash
# 1. 先自检扫描器本身：正例不误报、反例能被检出
bash scripts/read-only-guard.sh --self-test

# 2. 再扫描真实代码
bash scripts/read-only-guard.sh
```

扫描器自身执行失败（例如 awk 报错）时以**退出码 2** 失败。这一点是刻意的：
awk 崩掉会让输出为空，若把空输出当作「通过」，就会得到一个「假绿灯」——
这正是本项目已经踩过一次的坑。

### 四层校验

| 层 | 规则 | 拦住的典型写法 |
|----|------|---------------|
| 0 | **调用门槛**：语句须含 `Command::new("git")` 或 `self.run(`，否则不按 Git 规则校验 | 反向作用：避免把规则表 id（`public-api`）、扩展名（`rs` / `ts`）当成子命令误报 |
| 1 | **动词白名单**：语句内形如 `[a-z][a-z-]*` 的字面量必须是已知只读子命令；**动词之后的小写词按参数处理** | `push`、`commit`、`reset`；`remote get-url origin` 里的 `get-url` / `origin` 是参数，不报 |
| 2 | **危险选项黑名单**：`-x` / `--xxx` 形式的字面量不得命中危险表；其中 `--textconv` / `--filters` 属**安全选项**（会用上威胁 1 的转换器） | `git branch -D` 的 `-D`（分支名含 `/` 时单词白名单看不见） |
| 3 | **条件动词须带只读标志**：`stash` / `branch` / `tag` / `remote` / `worktree` / `config` 必须带 `--list` / `--get` 之类明确读标志 | `git config user.email X`（键含点、值大写，前两层都看不见） |
| 4 | **写动词黑名单（全局，不分语句）**：`push` / `commit` / `fetch` / `pull` / `rebase` / `reset` / `checkout` … 以引号字面量出现即报 | `let args = ["push"]; self.run(&args)` —— 命令与调用分开写，第 0 层门槛会放过它 |

第 4 层是第 0 层门槛的配套：一收一放，既消掉误报，又让漏报面比只做门槛更小。

### 已知边界

扫描器是**静态字面量分析**，不做数据流追踪：把命令拆进运行时拼装的字符串仍可绕过。
它的价值是拦住**无意之失**（尤其 AI 生成代码时的误用），而非抵御恶意提交。
真正的兜底仍然是人工审查与本项目的只读承诺。

另两条已知边界（都写进了脚本注释）：

- **不做词法分析**：注释里的 `"..."` 与代码一视同仁，在注释里给纯小写词加引号也会报违规。
- **第 4 层对任何语句生效**：面向用户的提示文案里若出现被引号包起来的写动词
  （如 `"push 被禁止"`）也会被拦。这是刻意选的偏向 —— 宁可让人改一句文案，也不放过一处写操作。

### 历史教训（保留，避免重蹈）

扫描器最初是一行 shell：

```bash
grep -rn 'Command::new("git")' crates/repo-prism-core/src/ \
  | grep -oE 'arg\("[a-z-]+"\)'
```

它**恒为空**：Rust 链式调用把 `.arg()` 写在后续行，只匹配同一行的 grep 抓不到任何东西；
同时会把 `--show-toplevel` 这类合法选项误判为子命令。
一个恒为空的安全检查比没有检查更危险 —— 它会让人误以为边界已被守护。
