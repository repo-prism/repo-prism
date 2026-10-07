# AGENTS.md — RepoPrism AI 协作协议

## 项目定位

RepoPrism（仓库棱镜）是一个**只读仓库智能工具**，为人类开发者和 AI Agent 提供代码库的多视图洞察。
核心隐喻：同一仓库，经过棱镜折射，呈现架构图、diff、文本化仓库、提交图、PR 状态等多种视图。

## 绝对禁止事项（只读宪法）

1. **绝不执行任何 Git 写操作**：`commit` / `push` / `pull` / `fetch` / `merge` / `rebase` / `reset` / `checkout` / `stage` / `tag` / `branch -d` / `stash pop` / `clean`
2. **绝不修改**被观察仓库的 `.git` 目录下任何文件
3. **绝不运行**被观察仓库定义的 external content filter 或转换器
4. **绝不下载** LFS 对象内容，只读取指针展示 object ID 和 size
5. **绝不执行**被观察仓库中的任何脚本、hook 或二进制

违反上述任何一条的 PR 一律拒绝合并，无论功能多好。

## 代码规范

- **Rust**：`cargo fmt` + `cargo clippy -- -D warnings`，零 warning
- **TypeScript**：Biome 格式化 + lint
- **文件命名**：kebab-case（如 `commit-graph.tsx`）
- **变量命名**：camelCase
- **组件命名**：PascalCase
- **提交信息**：Conventional Commits
    - `feat:` 新功能
    - `fix:` 修复
    - `docs:` 文档
    - `refactor:` 重构
    - `test:` 测试
    - `chore:` 杂项

## 目录职责

| 目录 | 职责 | 是否允许调用 Git |
|------|------|------------------|
| `crates/repo-prism-core` | Git 只读读取逻辑 | ✅ 唯一允许 |
| `crates/repo-prism-cli` | CLI 二进制 | ❌ 仅调用 core |
| `crates/repo-prism-mcp` | MCP Server | ❌ 仅调用 core |
| `src-tauri` | Tauri 应用命令层 | ❌ 仅调用 core |
| `src` | React 前端 | ❌ 不接触 Git |

## 任务工作流

1. 在 `TASKS/` 创建任务卡，包含：目标、上下文、约束、验收标准
2. 实现 + 自测
3. CI 必须全绿：fmt / clippy / lint / typecheck / test / 只读扫描 / 性能基准
4. 人类审查关键代码后合并

## 质量门禁

- **只读安全**：CI 静态扫描 `crates/repo-prism-core` 中的 Git 调用白名单
- **性能**：大仓库（Linux 内核级）首屏 < 3s，内存 < 300MB
- **跨平台**：macOS / Windows / Linux CI 矩阵

## 与 AI Agent 协作方式

- AI 代理只处理单张任务卡，不跨卡修改
- 每张任务卡必须包含可验证的验收标准
- AI 生成的代码必须附带测试
- 人类主导架构决策、命名、发布