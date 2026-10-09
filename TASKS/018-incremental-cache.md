# TASK-018: 增量加载与本地缓存 —— 引用映射的跨调用复用

**状态**：已完成（2026-10-09）
**手段范围说明**：血统（`005` 文档第八节）把本卡写作「增量加载与本地缓存（提交图、blame）」。
本卡交付**引用映射的跨调用缓存**（含它的失效判据与门禁），并**明确列出未覆盖的部分与理由**
（见「未覆盖的部分」一节）。这不是缩范围，而是把「缓存」这件必须先解决正确性问题的事
单独做完，再谈缓存什么。

## 目标

把「同一个仓库的连续多次读取」从「每条命令各读一次引用」变成「整个会话读一次」，
**且不允许出现静默过期** —— 仓库在两次调用之间被改了，界面必须跟着变。

验收标准同样放在**确定性的量**上（spawn 次数），理由见 `ADR/002` 第 2.5 节。

## 上下文：收益到底在哪里（实测，不是推断）

先纠正一个容易想当然的判断：**单个命令内部只会读一次引用映射**，
所以「实例内缓存」收益为零。收益**完全在命令之间**。

而桌面端打开一个仓库要跑**四条命令**（前端 `App.tsx` 里是 `inspect_repo` 之后
一个 `Promise.all`），其中三条都要引用映射：

| 命令 | 改动前的子进程 | 说明 |
|------|---------------:|------|
| `inspect_repo` | 3 | `open` 1 + `snapshot` 2（status + for-each-ref） |
| `get_commits` | 3 | `open` 1 + `commits` 2（log + **又一次 for-each-ref**） |
| `analyze_changes` | 5 | `open` 1 + `snapshot` 2（**又是一次 for-each-ref 与一次 status**）+ numstat 2 |
| `get_remote_info` | 2 | `open` 1 + remote 1 |
| **合计** | **13** | 同一份引用被读了 **3 次** |

点开一个提交（`get_commit_detail` 5 + `get_commit_diff` 3）= **8 次**，
其中 `open` 与引用各被重复一遍。

`ADR/002` 第六节留的钩子就是这件事：合并子进程后 `snapshot() + commits()` 是 4 次，
「想把两者共用（降到 3 次）需要跨调用缓存引用映射，那是 `TASK-018` 的范围」。
本卡交付的正是这一步。

## 约束

- **只读宪法不变**：不新增任何 `git` 子命令种类。缓存**只减少**调用次数，
  命令集合不扩大，`read-only-guard.sh` 的动词白名单一字未改。
- **缓存必须显式开启**：`Git::open` 的行为与逐次成本保持**逐字不变**，
  只有 `Git::open_cached` 才缓存。理由见下。
- 不得为缓存引入新的 crate 依赖（含 `lru` 之类的缓存库）。
- 缓存的有效性必须**可证伪**：每条失效路径都要有一条会失败的测试。

## 关键决策一：缓存是 opt-in，不是默认行为

如果让 `snapshot()` 默认走缓存，同一个方法就有了**冷/热两个 spawn 数字**，
而现有门禁 `perf.rs::spawn_counts_are_pinned` 钉的恰是「每次调用起几个进程」
这种**与状态无关**的量。缓存一旦默认开启，那条门禁会退化成「看测试跑的顺序」——
它就不再是契约，而是当前状态的快照。

所以：

| 构造 | 引用映射 | 用途 |
|------|---------|------|
| `Git::open` | **不缓存**，每次重读 | CLI（一次性）、既有测试、性能门禁 |
| `Git::open_cached` | 跨调用复用 | 桌面端会话（`src-tauri` 的 `RepoSession`） |

代价是缓存有了独立的一组测试；收益是既有的门禁**一个字都不用改**，
`spawn_counts_are_pinned` 的语义完全没有被削弱。

## 关键决策二：失效判据用「内容指纹」，不用文件时间戳

缓存唯一不可接受的失效是**过期了却没人知道**。文件时间戳不足以判定：

- 多数文件系统的 mtime 粒度到秒；
- `git update-ref` 改写一个**已存在**的松引用时，文件大小也不变
  （内容恒为 40/64 位十六进制 + 换行）。

于是改为**直接哈希 git 用来解析引用的那几份数据本身**：

| 参与指纹 | 为什么必须有它 |
|---------|--------------|
| `<git-dir>/HEAD` | 「当前分支」标记（`%(HEAD)`）的唯一来源；它既不在 `refs/` 下也不在 `packed-refs` 里 |
| `<common-dir>/packed-refs` | 全部引用已打包时，**删一个分支只会重写这个文件**，`refs/` 下始终为空 |
| `<common-dir>/refs/{heads,tags,remotes}/**` 的**相对路径** | 两条分支指向同一提交时，松引用**内容逐字节相同**，只有文件名能区分改名 |
| 上述文件的**内容** | 分支被移回旧提交时文件名与大小都不变，只有内容变 |

`HEAD` 取自 **per-worktree** 的 git 目录，`refs/` 与 `packed-refs` 取自 **common** 目录 ——
链接工作树下这两者不同（实测：`--git-dir` 是 `.git/worktrees/<name>`，
`--git-common-dir` 是 `.git`）。为此 `open()` 的 `rev-parse` 改为一次取三行
（`--show-toplevel` / `--absolute-git-dir` / `--git-common-dir`），**仍是 1 次子进程**。

预算 4 MiB：引用数据总量超过即**放弃缓存、每次重读**。读失败同样是这个方向。
**退化的方向永远是「多起一个子进程」，不是「用一份可能过期的数据」。**

## 关键决策三：会话放在宿主，不进库的全局状态

缓存要跨命令生效，就必须有东西活过单个命令。选择让 **`src-tauri` 的 `AppState`
持有一个 `RepoSession`**，而不是在 core 里放 `static`：

- 库不引入全局可变状态（测试之间也就不会有隐式耦合）；
- 「缓存活多久」这件事变成一行看得见的代码，而不是藏在库里；
- 路径不匹配即整体替换会话（顺带丢掉旧缓存），换仓库不会串味。

命中判断同时看**本次入参**与**已解析出的仓库根**：前端先用用户输入的路径调
`inspect_repo`，之后改用返回的 `snapshot.path` 调其余命令 —— 只看输入字符串会必然落空一次。

## 未覆盖的部分（如实列出）

| 未做 | 理由 |
|------|------|
| **blame 的缓存** | 血统里写作「（提交图、**blame**）」，但 `git blame` 这个**功能本身**尚未实现（是产品能力，需要 SPEC 用户故事 + UI 面板）。缓存一个不存在的功能没有意义。`blame` 已在只读白名单的动词表里，实现时可直接套用本卡的缓存机制 |
| **提交详情 / diff 的内容寻址缓存** | 理论上 sha → 内容是 git 的不变量，缓存是安全的。但它引入一个**新的失效面**：`.gitattributes` 的 diff driver 改动会让同一 sha 的 patch 文本变化，而这条边界**目前没有任何测试覆盖**。在本项目里，「引入一个自己无法证明其失效判据的缓存」比「暂时不缓存」更危险，故留待有对应测试之后再谈 |
| **提交图分页结果的缓存** | 结果依赖 `HEAD` 与全部引用；指纹可以复用本卡的机制，但当前前端只在打开与滚动时各取一页，重复调用的收益未经测量。**先测再优化** |
| **MCP server 的会话复用** | MCP 是长驻进程，连续的工具调用本可复用引用 —— 但它要的是**按路径索引的会话表**（一次会话可能服务多个仓库），而那正是 TASK-019 的形状。本卡只在桌面端做「一次一个会话」，不提前把多仓库的形状定死 |
| **CLI 的复用** | CLI 每次进程只执行一条命令，缓存对它没有任何收益。故 `crates/repo-prism-cli` 刻意保留 `Git::open` |
| **毫秒级的收益承诺** | 同 `ADR/002` 第 2.5 节。本卡只承诺 spawn 次数 |

## 实现

| 文件 | 改动 |
|------|------|
| `crates/repo-prism-core/src/git.rs` | `Git` 增加 `git_dir` / `common_dir` / 可选的 `refs_cache`；`open()` 与 `open_cached()` 分家；`refs()` 拆成「带缓存的包装」与 `read_refs()`；新增纯函数 `ref_fingerprint` / `hash_file` / `collect_files` |
| `crates/repo-prism-core/tests/cache.rs` | **新增 12 项**：1 条「缓存确实命中」+ 7 条「引用一变必须失效」+ 1 条「工作区状态从不走缓存」+ 1 条「不开启缓存时行为不变」+ 1 条链接工作树 + 1 条「改名也要失效」 |
| `crates/repo-prism-core/tests/scale.rs` | 新增第六节：把「不缓存 4 次 / 开缓存 3 次」做成可复跑的对照，并**明确标注毫秒差不可作结论** |
| `crates/repo-prism-core/tests/perf.rs` | 门禁数字未改。补写「为什么钉的是 `Git::open`」的说明 |
| `src-tauri/src/lib.rs` | 新增 `RepoSession` 与 `session_git` / `with_git`；七条仓库命令改走会话；`analyze` 保留 `Result<_, String>` 签名以免新增 `anyhow` 依赖边 |

**前端一行未改**：命令名与参数不变。

## 验收标准

- [x] `snapshot() + commits()` 的子进程次数 **4 → 3**（`ADR/002` 承诺的那一步）
- [x] 打开仓库的四条命令：**13 → 8** 次子进程（`open` 只 1 次 + 引用只读 1 次）
- [x] 缓存**显式开启**，`Git::open` 的逐次成本与改动前逐字相同
      —— `perf.rs::spawn_counts_are_pinned` **一字未改且仍通过**
- [x] 7 条失效路径各有测试，且都同时断言「结果正确」与「确实重读了」：
      新建分支 / 新建标签（并验到提交角标）/ 删除分支 / 分支改名 /
      分支被移回旧提交 / 松引用被打包 / **删除一个已打包的引用**
- [x] 链接工作树下同样正确（`refs` 取自 common 目录，`HEAD` 取自 per-worktree 目录）
- [x] 工作区状态**从不**走缓存（改文件后 `snapshot()` 立即反映，且只起 `status` 一次）
- [x] **反证已实测**：6 个探针逐个破坏实现，6/6 被对应用例抓住（见下）
- [x] 只读扫描器通过，动词白名单未改
- [x] 全量 144 passed（core 116 / CLI 14 / MCP 14）

## 反证记录（本项目规矩：不会失败的测试比没有测试更危险）

`/private/tmp/probe_p09.py` 逐个破坏 `git.rs`，跑 `--test cache`，再还原：

| 探针 | 破坏方式 | 被抓住的用例 |
|------|---------|-------------|
| A | 指纹恒为常数 | 7 条失效用例同时红（4 passed / 8 failed） |
| B | 指纹永远算不出来（缓存从不生效） | `a_cached_instance_reads_the_ref_map_once` 等 3 条 |
| C | 指纹漏掉 `HEAD` | `switching_the_checked_out_branch_…` |
| D | 指纹漏掉 `packed-refs` | `deleting_an_already_packed_ref_…` |
| E | 指纹漏掉相对路径 | `renaming_a_branch_…` |
| F | 指纹只记名字、不记内容 | `moving_a_branch_to_another_commit_…` |

**探针 D 第一次是「没被抓住」的**，这暴露了一条真实的洞：`packing_the_refs` 那条用例
实际靠的是「松引用文件消失」，而**引用本来就已打包**时删掉一个分支只重写 `packed-refs`，
`refs/` 集合完全不变 —— 那时指纹若不覆盖 `packed-refs` 就会静默过期。
补上 `deleting_an_already_packed_ref_invalidates_the_cached_ref_map` 之后 D 才被抓住。
**这条洞是被反证逼出来的，不是设计时想到的。**

## 与归档规划的偏离（必须记录）

`005` 文档第八节写的是「增量加载与本地缓存（提交图、blame）」。本卡：

1. 交付了**引用映射的跨调用缓存**（缓存机制 + 失效判据 + 门禁），这是「本地缓存」里
   唯一能立即兑现且正确性可证的那一块；
2. **blame 未做** —— 它是尚未实现的产品能力，不是缓存能凭空补上的；
3. 提交详情 / 分页结果的缓存**列明了未做的理由**，不是遗漏。

卡号与主题（增量加载与本地缓存）未变。

## 验证记录（本机 macOS，2026-10-09）

```text
cargo test -p repo-prism-core              → lib 53 + 5 + 10 + 2 + 5 + 16 + 8 + 1 + 4 + 12 = 116 passed
cargo test -p repo-prism-cli               → 14 passed
cargo test -p repo-prism-mcp               → 14 passed
cargo test -p repo-prism-core --test cache → 12 passed
cargo fmt / clippy --all-targets -- -D warnings → 干净
bash scripts/read-only-guard.sh            → Read-only guard passed（自检 26/26）
node scripts/check-versions.mjs            → next version: 0.2.0
```

`tests/scale.rs` 第六节实测（4 万提交，加载轮，**仅示量级**）：
不缓存 4 次 spawn 201ms / 开缓存 3 次 179ms；差 22ms **小于同一次运行里单次 spawn
的固定成本 46–58ms**，因此毫秒差不可作结论，可靠结论只有「4 → 3 次」。
