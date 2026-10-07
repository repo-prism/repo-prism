# TASK-003: 实现 CLI `repoprism inspect --json`

## 目标
提供 CLI 命令输出 JSON 快照。

## 上下文
- CLI 调用 `repo-prism-core`
- 输出为稳定 JSON schema

## 约束
- 不引入重量级 CLI 框架（用 `clap` 即可）
- JSON 输出可被 Agent 直接解析

## 验收标准
- [ ] `repoprism inspect . --json` 输出合法 JSON
- [ ] schema 与 SPEC.md 数据模型一致
- [ ] `--help` 显示用法
- [ ] 集成测试：在临时仓库上运行并校验 JSON

## 禁止事项
- 不得在 CLI 中直接调用 Git