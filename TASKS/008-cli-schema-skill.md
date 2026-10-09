# TASK-008: CLI 规范化 JSON Schema + Skill 打包

**状态**：已完成（2026-10-09）

## 目标

1. CLI `inspect --json` 输出带 `schema_version` 的稳定结构
2. `repoprism commits` / `repoprism detail` 两个子命令
3. `repoprism skill --path` 输出 Skill 目录路径
4. `repoprism skill --print` 直接打印 Skill 内容

## 上下文

- 依据：原规划文档 `002`（TASK-007 → 009 批次预告）与 `003`（本卡正文）；对应 US-4 / US-5
- 三条数据命令共用同一个信封，Agent 侧只需实现一次解析
- Skill 内容以 `include_str!` 内联进二进制，随版本一起分发

## 约束

- 不引入重量级 CLI 框架（`clap` 足够）
- JSON 信封字段只增不减，不做静默改名
- Skill 只能落到**本工具自己的**缓存目录，绝不写入被观察仓库
- Skill 展开必须幂等：内容相同不重写

## 验收标准

- [x] `inspect --json` 顶层包含 `schema_version`、`tool`、`tool_version`、`data`
- [x] `commits <path> --limit N --json` 输出提交数组，`--limit` 上限 2000
- [x] `detail <path> --sha <sha> --json` 输出 `info` + `files` + `patch` + `truncated`
- [x] `skill --path` 首次调用自动展开 Skill 到缓存目录，重复调用幂等
- [x] `repoprism --version` 可用
- [x] Skill 内容对 Agent 清晰可读（含信封说明、字段语义、截断与合并提交的注意事项）
- [x] CI 只读扫描通过

## 实现

- 新增 `skill/SKILL.md`（编译期内联，`repoprism skill --print` 与磁盘内容同源）
- 重写 `crates/repo-prism-cli/src/main.rs`：`inspect` / `commits` / `detail` / `skill`
- `crates/repo-prism-cli/Cargo.toml`：新增 `serde`、`dirs = "5"`
- 新增集成测试 `crates/repo-prism-cli/tests/commands.rs`（8 个用例，全部走真实子进程）
- 既有 `tests/inspect.rs` 改为读信封内的 `data`

## 偏离说明

1. **`REPOPRISM_CACHE_DIR` 环境变量**：原规划直接用 `dirs::cache_dir()`。
   这里加了一层环境变量覆盖，唯一目的是让测试能把 Skill 展开到临时目录，
   避免测试污染开发机的真实缓存目录。生产路径行为不变。
2. **`dirs = "5"`**：按原规划保留（`dirs 7` 已在本机 registry，但没必要为此改版本）。

## 禁止事项

- 不得在 CLI 中直接调用 Git（只允许经 `repo-prism-core`）
- 不得把 Skill 写到被观察仓库内
