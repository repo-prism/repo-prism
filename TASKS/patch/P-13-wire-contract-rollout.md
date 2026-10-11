# P-13：把线格式闸门推广到全部跨语言模型

- **状态**：✅ 完成（2026-10-11，提交见文末）
- **类型**：工程补丁（把 P-12 的一次性机制变成通用机制）
- **对应 US**：横跨 US-1（仓库状态）/ US-2（变更）/ US-4（CLI）/ US-6（MCP）/ US-7（风险）

## 目标

P-12 只管住了 `BlobPreview` 一个模型。剩下的 `RepoSnapshot` / `WorkspaceInfo` /
`ChangeAnalysis` 是**同一类风险**：JSON 形状由 Rust 的 serde 决定，前端那份是手抄的，
两边没有任何编译期联系。首屏读的就是 `RepoSnapshot` —— 它漂了，界面直接变空白。

这次要的是**机制推广**，不是「再手抄三份」：加一个新模型受管应当是**加数据**，
不是复制一遍生成器与解析器。

## 上下文（为什么不能照抄 P-12）

P-12 的做法是：Rust 侧为 `BlobPreview` 手写一份 contract 生成器，TS 侧手写一份
字段清单 + 一份解析器。对 1 个模型成立；对 4 个模型，手抄量是 4 倍，而且
**第 4 份抄错的概率远高于第 1 份** —— 这正是要用机器替代手抄的理由。

所以 P-13 的关键决策是**把形状推导做成通用的**。

## 关键决策

1. **形状由样本推导，不由人声明**。每个模型只提供：
   - 一个「丰满样本」（每个 `Option` 都是 `Some`、每个 `Vec` 至少 1 个元素）
   - 一个反序列化探针（用于判定 nullable，见下）
   - 可选的枚举清单 / tagged-union 变体清单（serde 无运行时反射，这两样只能手写）
   其余（字段集合、每字段的 JSON 类型、嵌套形状、数组元素形状）全部推导。

2. **nullable 用反序列化探测，不用第二个样本**。把某个路径上的值换成 `null`
   再 `from_value::<T>()`：成功 ⇒ 该字段可空。判据是**serde 自己说的**，
   于是「Rust 把 `String` 改成 `Option<String>`」这种漂移会被 contract 抓到，
   而不是靠人记得同步一份清单。

3. **TS 侧不手抄形状，只手抄「字段名」**。运行时校验直接吃 `contracts/*.json`
   （import 进来就是判据），手抄的只有一份
   `Record<keyof T, true>` 的键表 —— 漏一个字段 / 多一个字段都是**编译错误**，
   再拿它和 contract 的 `fields` 比对。这样：
   - TS 改了 interface ⇒ 编译红 或 这里红
   - Rust 改了形状 ⇒ contract 变 ⇒ 这里红
   - 运行时收到的 JSON 与 contract 不符 ⇒ `WireContractError`，界面显示一行提示
     （比静默渲染出空白好排查；三个调用点都有 `catch`）

4. **tagged union 靠登记，且有启发式兜底**。推导器遇到对象字段若含 `kind` 键
   而该路径**没有**登记为 union，直接 panic 提示去登记 —— 免得「新增一个 union
   但没人登记」静默退化成普通对象（那样 props 集合就不再受管）。

5. **不引入 `specta` / `ts-rs`**（沿用 P-12 决策 2）。升级条件见本文末节。

6. **samples 保留 `expect_tag`**，仅对含 union 的模型有意义；并保留
   「每个 union 变体至少有一个样本」这条断言 —— 少了它，逐样本用例会漏掉某一类。

## 验收标准

- `cargo test -p repo-prism-core --test wire_contract` 绿（4 份 contract 各一条比对用例）
- `vitest run` 绿；`wire.test.ts` 对着 4 份 contract 断言：字段集合 / 枚举取值 /
  union 变体 / 每样本可解析 / 解析器拒绝异常形状
- 反证探针：改 Rust 字段名、加字段、`String`→`Option<String>`、改枚举、改 union 变体、
  改 TS 键表 —— **每个方向都确实红**，各跑 3 轮
- `inspectRepo` / `getWorkspace` / `analyzeChanges` 三个运行时入口都过严格解析
- 只读扫描器、fmt、clippy、biome、tsc、build、版本闸门全绿；远端 CI 全平台绿

## 禁止事项

- 不允许为了「让 contract 好生成」去改生产代码的序列化形状（改形状必须是有意的产品决策）
- 不允许 contract 文件里出现**平台相关**的值（真实路径、行尾、locale）——
  P-12 已经在 Windows 上因为 CRLF 红过一次
- 不允许把校验只放在测试里（那就回到「清单是注释」）
- 不允许 CI 自动更新 contract（沿用 `#[ignore]` 的重生成约定）

## 实际改动

| 文件 | 改动 |
|------|------|
| `crates/repo-prism-core/tests/wire_contract.rs` | 重写。从「只为 `BlobPreview` 生成」变成通用推导器（路径定位 / 形状推导 / 可空性探测 / union 注入），四个模型各交一份样本 + 探针 + 可选的枚举与 union 清单 |
| `contracts/snapshot.json` / `workspace.json` / `analysis.json` | **新增** |
| `contracts/blob-preview.json` | 走同一套推导器后重新生成（顶层 `fields`/`kinds`/`image_formats` 收敛成 `shape` + `constants`） |
| `src/lib/wireContract.ts` | **新增**。通用解析器 `parseWire` + `WireContractError` + `KeyMap` / `fieldsOf` / `shapeAt` / `VariantSpec` |
| `src/lib/snapshot.ts` / `workspaceInfo.ts` / `analysis.ts` | **新增**。受管模型的类型搬出 `api.ts`，附键表与 `parseXxx` |
| `src/lib/blobPreview.ts` | 手写解析器换成通用解析器；`BLOB_KIND_KEYS` → `BLOB_KIND_VARIANTS`（变体可带自己的枚举） |
| `src/lib/wire.test.ts` | **新增**，72 项。四个模型共用一套断言（字段集合 / 枚举 / union / 样本解析 / expect_tag / 逐层破坏） |
| `src/lib/api.test.ts` | **新增**，6 项。证明三个入口**确实**走严格解析（不是只在测试里） |
| `src/lib/api.ts` | 受管类型改为 re-export；`inspectRepo` / `getWorkspace` / `analyzeChanges` 过 `parseXxx` |
| `src/lib/blobPreview.test.ts` | 收敛到「上限」这一个 contract 管不到、也不该由它派生的量（通用断言移入 `wire.test.ts`） |

## 探针（9 个方向，逐个实测）

「会红」这件事必须逐个证明 —— 一条没被执行过的断言等于没有断言。

| # | 破坏 | 谁红 | 结果 |
|---|------|------|------|
| P1 | Rust 给 `HeadInfo.commit` 加 `#[serde(rename)]` | Rust 红（定位到第 21 行）→ 重生成 → `wire.test.ts` 红 | ✅ |
| P2 | `WorktreeInfo.branch` 从 `Option<String>` 改成 `String` | Rust 红（`nullable` 少一项） | ✅ |
| P3 | 给 `TagInfo` 加一个字段 | Rust 红 | ✅ |
| P6 | 给 `ChangeKind` 加变体**但不同步**取值清单 | **不红**（已知缺口，坐实） | ⚠️ |
| P6b | 同上但同步了清单 | Rust 红 → 重生成 → `wire.test.ts` 红 4 条 | ✅ |
| P7 | 给 `RepoSnapshot` 加一个**未登记**的 tagged union 字段 | 生成时 panic「看着像 tagged union 却没有登记」 | ✅ |
| T1/T5 | TS 键表漏字段 / 枚举清单漏取值 | `tsc` 编译错误 | ✅ |
| T8 | TS `interface` 加字段而键表不动 | `tsc` 编译错误（反向也成立） | ✅ |
| T7 | 把 `inspectRepo` 的 `parseRepoSnapshot` 摘掉 | `api.test.ts` 红 | ✅ |
| T9 | 把 contract 转成 CRLF | 仍绿（两道防线：`.gitattributes` + 归一化） | ✅ |

### 探针暴露的两个真问题（都已修）

1. **丰满样本没写满不会报警**：`BlobPreview` 的样本三个 `Option` 全是 `None` 时，
   类型被推成 `"null"` —— 闸门只知道「可空」，不知道「不空时是什么」。
   现在丰满样本里出现 `null` 直接 panic。空数组同理（更早就有这条）。
2. **报错只报第一行不够**：`nullable` 少一项时，第一处不同的行是 `"nullable": [`
   这种容器开头，真正的差别在下面。现在报**三处**（P2 就是这么被看出来的）。

## 与归档的偏离

无。本卡没有对应归档条目，是 P-12 的横向推广 —— 归档 001–006 里没有任何一条
要求「跨语言契约」，P-12/P-13 都是在补「Rust 绿而 UI 空白」这类**测试盲区**。

## 升级到 `specta` / `ts-rs` 的条件

满足任一即应评估替换：

1. 受管模型超过 ~8 个，手写样本本身开始成为负担；
2. 需要校验**字段类型**而不只是字段集合（当前 TS 的 interface 里
   `string` / `number` 仍是手抄的，contract 不校验它）；
3. 出现了泛型容器或自定义 `serialize_with`，样本推导开始漏形状。
