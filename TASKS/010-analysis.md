# TASK-010: 变更分析与风险标记

**状态**：已完成（2026-10-09）
**血统**：批次 `004`（原规划 `RepoPrism-仓库棱镜-004`）

## 目标

1. 结构化解析 diff（hunks、文件、增删行）
2. 启发式风险规则引擎（纯本地，无网络）
3. 可选 LLM 摘要层（trait 抽象，默认 Noop）
4. Tauri 命令 + 前端风险角标

## 验收标准

- [x] `analyze_changes` 返回 summary + per-file risk
- [x] 至少 8 条风险规则，覆盖异常处理、迁移、API、配置、CI、依赖、测试、大量删除
- [x] 前端在变更文件中显示风险角标
- [x] CI 只读扫描通过

## 实现

| 文件 | 内容 |
|------|------|
| `crates/repo-prism-core/src/analysis.rs`（新增） | `RiskLevel` / `Risk` / `RiskCounts` / `ChangeAnalysis` / `ChangeGroup` / `ChangeFacts` / `Rule` / 规则表 / `Summarizer` + `NoopSummarizer`；16 个单测 |
| `crates/repo-prism-core/src/model.rs` | `LineStat` + `LineStats`（工作区行数统计，**不属对外 JSON 契约**） |
| `crates/repo-prism-core/src/diffparse.rs` | `parse_numstat`（解析 `git diff --numstat -z`）+ 3 个单测 |
| `crates/repo-prism-core/src/git.rs` | `working_tree_stats()`；两次 `--numstat` 调用 |
| `crates/repo-prism-core/src/lib.rs` | 导出 `analysis` 模块（保留 `mod diffparse`） |
| `crates/repo-prism-core/tests/analysis.rs`（新增） | 5 个集成测试，真实仓库端到端 |
| `src-tauri/src/lib.rs` | 新增 `analyze_changes` 命令（保留既有五个命令） |
| `src/lib/api.ts` | `RiskLevel` / `Risk` / `RiskCounts` / `ChangeAnalysis` + `analyzeChanges` |
| `src/lib/risk.ts`（新增） | `groupRisksByPath` / `topRisk` / `riskTooltip` / `riskLevelText`，纯函数 |
| `src/lib/risk.test.ts`（新增） | 9 个用例 |
| `src/components/ChangesPanel.tsx` | 摘要条 + 关键/警告计数 pill + 逐行风险角标（**合并式追加**，未改动原有分组渲染） |
| `src/App.tsx` | 摘要与远端并行拉取（**保留**详情/diff 并行加载与 `main-split` 布局） |
| `src/App.css` | 摘要条、pill、角标样式 |

## 规则表（10 条）

| rule_id | 等级 | 判定 | 对应验收类别 |
|---------|------|------|-------------|
| `conflict-in-progress` | critical | 分组为冲突 | — |
| `env-or-secret` | critical | `.env` / `.env.*` / `*.pem` / `*.key` / 名含 secret、credential | 配置 |
| `migration-file` | critical | `migrations/` / `migration/` / `*.sql` / `schema/` | 迁移 |
| `ci-config` | warn | `.github/workflows/` / `.gitlab-ci*` / jenkinsfile / `.circleci/` | CI、配置 |
| `dependency-manifest` | warn | `Cargo.toml` / `package.json` / `pnpm-lock.yaml` / `go.mod` … | 依赖、配置 |
| `test-deleted` | warn | 测试路径 + 类型为删除 | 测试 |
| `mass-deletion` | warn | 删除 ≥ 100 行且 > 新增 × 3（**需行数统计**） | 大量删除 |
| `error-handling` | info | 文件名含 error / exception / panic | 异常处理 |
| `public-api` | info | 源码扩展名且非测试路径 | API |
| `lockfile-only` | info | `*.lock` / `*-lock.json` / `*-lock.yaml` | 依赖 |

## 与归档规划的两处偏离（均为了让验收标准真正成立）

### 1. 引入 `ChangeFacts` 与可选行数统计

归档给出的规则表只有 8 条，但同一条验收标准要求覆盖「**大量删除**」。
大量删除必须知道行数，而 `FileChange` 只有 `{path, kind}`。

于是把工作区 diff 的行数统计作为**可选**输入带进规则：
`Git::working_tree_stats()` 跑两次 `git diff --numstat -z`（已暂存 + 未暂存），
按路径建键（重命名取新路径，与 `status --porcelain=v2` 报的路径一致）。

- `ChangeAnalysis::from_status(status)` —— 无行数，`mass-deletion` **不命中**（不误报）
- `ChangeAnalysis::from_status_and_stats(status, stats)` —— 有行数，可命中

这两条路径都有测试断言（`without_stats_only_path_rules_fire`）。

归档里「结构化解析 diff（hunks、文件、增删行）」这一条目标也正是为此 ——
`diffparse.rs` 早已具备结构化解析能力，本卡才真正把它用上。

### 2. `public-api` 判定改写

归档原文 `(p.ends_with(".rs") && p.contains("/src/"))` 对 `src/a.rs`
这类**顶层路径恒为假**（没有前导斜杠），且只排除 `.test.`，漏掉 `.spec.*` 与 `tests/`。

改为：按源码扩展名判定 + 共享的测试路径判定 `is_test_path`。
`public_api_covers_top_level_src_paths` 就是为这条写的回归测试。

### 顺带修掉的一个真实 bug

归档口径是「先 `to_lowercase()` 整条路径再取 basename」，实现时漏了小写化，
`Cargo.toml` 对不上小写清单里的 `cargo.toml`，`dependency-manifest` 恒不命中。
现已补上 `basename_lower` 并有 `rule_matching_is_case_insensitive` 覆盖。

## 安全与只读

- 分析层**纯本地**：不调用任何外部 API、不访问网络
- `Summarizer` trait 默认实现是 `NoopSummarizer`（恒返回 `None`），
  「本地优先」是不需要配置的默认行为
- 两次 `--numstat` 都显式带 `--no-textconv`（`SECURITY.md` 威胁 1）
- 新增 Git 调用已通过扫描器（本卡顺带催生了 [P-05](patch/P-05-guard-scope-and-write-verbs.md)，因为 P-02 的规则 1 门槛过宽会误报本卡新增的小写词字面量）

## 不属本卡范围（拆给后续）

| 事项 | 归属 |
|------|------|
| 真正接入 LLM（本地 Ollama） | TASK-013 |
| CLI / MCP 暴露分析结果 | TASK-015（MCP 工具扩展），SPEC US-4 未列入 `analyze` 子命令 |
| 提交级（而非工作区级）分析 | TASK-014 |

## 验证记录（本机 macOS，2026-10-09）

- `cargo test -p repo-prism-core`：**75 passed**
  （lib 38 / analysis 5 / diff 10 / perf 1 / remote 5 / snapshot 16）
- `cargo test -p repo-prism-core --lib analysis`：16 passed
- 前端：`biome check src` 干净、`tsc --noEmit` 干净、`vitest run` 31 passed（新增 9+5）
- `bash scripts/read-only-guard.sh`：passed；`--self-test` 26/26
- 手工核对：`snapshot()` 性能未受影响（本卡不碰 `snapshot()` 路径）
