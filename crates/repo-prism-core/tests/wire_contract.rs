//! 跨语言线格式 contract。
//!
//! # 为什么要有这个文件
//!
//! 这些模型的 JSON 形状是**三处**消费者的共同契约：桌面端 UI、CLI、MCP。
//! Rust 侧的形状由 serde 的 derive 决定，而前端 `src/lib/*.ts` 里那份类型是
//! **手抄**的 —— 两边没有任何编译期联系。
//!
//! 于是存在一类会导致「Rust 测试全绿、UI 静默变空白」的改动：改一个
//! `rename_all`、改一个字段名、加一个字段、把 `String` 改成 `Option<String>`。
//! 单测测的是 Rust 结构，而跨语言消费者拿到的是 JSON ——
//! 这两者此前没有任何东西把它们连起来。
//!
//! # 怎么连起来
//!
//! `contracts/*.json` 是唯一事实来源，两侧都去对它：
//!
//! ```text
//!   serde derive  ──生成──▶  contracts/<model>.json  ◀──读──  src/lib/wire.test.ts
//!        │                            ▲                                 │
//!        └── 改了 ⟹ 本文件红 ────────┘                                 └── TS 键表改了 ⟹ 那边红
//! ```
//!
//! - Rust 改了形状 ⇒ **这里**红，提示去重新生成
//! - TS 改了键表 / 类型 ⇒ `src/lib/wire.test.ts` 红
//! - 两边同时改但不一致 ⇒ 两侧都红
//!
//! # P-13：从「一个模型」推广到「一套机制」
//!
//! P-12 只管 `BlobPreview`，且生成器与解析器都是**为它手写**的。照抄四遍的话，
//! 「第 4 份抄错」的概率远高于第 1 份 —— 而抄错的地方正是闸门本身。
//!
//! 所以这里的关键是把**形状推导**做成通用的：每个模型只交出
//!
//! 1. 一个**丰满样本**（每个 `Option` 都是 `Some`、每个数组至少 1 个元素）；
//! 2. 一个反序列化探针（用来判定某个路径可不可为 `null`）；
//! 3. 可选的枚举取值清单 / tagged union 变体清单（serde 没有运行时反射，
//!    这两样只能手写 —— 见 [`Model`] 上的注释）。
//!
//! 字段集合、每字段的 JSON 类型、可空性、嵌套形状、数组元素形状全部由样本推导。
//!
//! ## 可空性为什么不写在清单里
//!
//! 判据是**把该路径换成 `null` 再问 serde 能不能反序列化**。成功即代表
//! 「线上这里真的会出现 null」，而不是「我记得它是 Option」。
//! 于是「Rust 把 `String` 改成 `Option<String>`」会自动改变 contract，
//! 不需要任何人记得同步一份清单。
//!
//! # 怎么更新
//!
//! ```bash
//! cargo test -p repo-prism-core --test wire_contract -- --ignored
//! ```
//!
//! 那条 `--ignored` 的用例会重写全部 contract 文件。**它是被 ignore 的，
//! 所以 CI 不会替你更新**：形状变了必须由人明确执行这条命令，并把 contract
//! 文件的改动连同两侧适配一起提交。这跟 `tests/scale.rs` 的诊断基准是同一个约定。
//!
//! # contract 里不能有平台相关的值
//!
//! Windows runner 的默认 `autocrlf` 会在 checkout 时把 LF 转成 CRLF，而
//! `serde_json` 生成的永远是 LF —— 逐字节比对会红，还会假装是「形状变了」
//! （**P-12 在 `0705e46` 上真实发生过**，macos / ubuntu 全绿、只有 windows 红）。
//! 根由由仓库根的 `.gitattributes` 治，本文件再做一层归一化兜底。
//!
//! 同理，样本里不能出现真实路径：它们在不同平台上长得不一样。

use repo_prism_core::{
    BlobKind, BlobPreview, BranchInfo, ChangeAnalysis, ChangeKind, FileChange, HeadInfo,
    ImageFormat, RepoSnapshot, RepoState, Risk, RiskCounts, RiskLevel, StashInfo, StatusInfo,
    TagInfo, UpstreamInfo, WorkspaceInfo, WorktreeInfo, MAX_PREVIEW_BYTES,
};
use serde_json::{from_value, json, to_value, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// tagged union 的标签字段。本项目两个 tagged enum 都用它。
const UNION_TAG: &str = "kind";

// ---------------------------------------------------------------------------
// 路径：在样本的 JSON 里定位一个字段
// ---------------------------------------------------------------------------

/// 路径语法是点分的，数组元素写作 `name[]`（取**首**元素）。
///
/// 只取首元素就够了：同一数组里所有元素的形状相同，而我们要的是形状，
/// 不是穷举它的内容。
#[derive(Clone, Debug)]
enum Seg {
    Field(String),
    Elem(String),
}

impl Seg {
    fn name(&self) -> &str {
        match self {
            Seg::Field(name) | Seg::Elem(name) => name,
        }
    }
}

fn split_path(path: &str) -> Vec<Seg> {
    path.split('.')
        .filter(|part| !part.is_empty())
        .map(|part| match part.strip_suffix("[]") {
            Some(name) => Seg::Elem(name.to_string()),
            None => Seg::Field(part.to_string()),
        })
        .collect()
}

fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for seg in split_path(path) {
        let parent = current.as_object()?;
        let child = parent.get(seg.name())?;
        current = match seg {
            Seg::Field(_) => child,
            Seg::Elem(_) => child.as_array()?.first()?,
        };
    }
    Some(current)
}

/// 把路径上的值换成 `value`，返回一个**新的** JSON（不改动入参）。
fn set_path(root: &Value, path: &str, value: Value) -> Value {
    set_segs(root, &split_path(path), value)
}

fn set_segs(node: &Value, segs: &[Seg], value: Value) -> Value {
    let Some((seg, rest)) = segs.split_first() else {
        return value;
    };
    let key = seg.name();
    let mut obj = node
        .as_object()
        .unwrap_or_else(|| panic!("路径 `{key}` 的前一段不是 JSON 对象"))
        .clone();
    let child = obj
        .get(key)
        .unwrap_or_else(|| panic!("样本里缺少键 `{key}`"))
        .clone();

    let replacement = match seg {
        Seg::Field(_) => set_segs(&child, rest, value),
        Seg::Elem(_) => {
            let mut items = child
                .as_array()
                .unwrap_or_else(|| panic!("`{key}[]` 要求 `{key}` 是数组"))
                .clone();
            let first = items
                .first()
                .cloned()
                .unwrap_or_else(|| panic!("`{key}[]` 要求 `{key}` 至少有一个元素"));
            items[0] = set_segs(&first, rest, value);
            Value::Array(items)
        }
    };
    obj.insert(key.to_string(), replacement);
    Value::Object(obj)
}

// ---------------------------------------------------------------------------
// 形状推导
// ---------------------------------------------------------------------------

/// 一个 JSON 值在**非空**时的类型名。与 TypeScript 侧的 `JsonType` 对应。
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn sorted(values: &[Value]) -> Vec<Value> {
    let mut out = values.to_vec();
    out.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
    out
}

fn child_path(path: &str, field: &str) -> String {
    if path.is_empty() {
        field.to_string()
    } else {
        format!("{path}.{field}")
    }
}

fn find_enum<'a>(model: &'a Model, path: &str) -> Option<&'a Vec<Value>> {
    model
        .enums
        .iter()
        .find(|(at, _)| *at == path)
        .map(|(_, values)| values)
}

fn find_union<'a>(model: &'a Model, path: &str) -> Option<&'a Variants> {
    model
        .unions
        .iter()
        .find(|(at, _)| *at == path)
        .map(|(_, variants)| variants)
}

/// 推导一个对象（或数组元素）的形状。
///
/// `root` 必须是**完整**文档的 JSON —— 可空性探测要把它整个交给 serde 试一遍，
/// 只给片段会让探测失去意义（片段自己永远能反序列化）。
fn derive_object(root: &Value, path: &str, model: &Model) -> Value {
    let node = get_path(root, path).unwrap_or_else(|| panic!("样本里没有路径 `{path}`"));
    let obj = node
        .as_object()
        .unwrap_or_else(|| panic!("路径 `{path}` 不是 JSON 对象"));

    let mut fields: Vec<String> = obj.keys().cloned().collect();
    fields.sort();

    let mut types = Map::new();
    let mut nullable: Vec<Value> = Vec::new();
    let mut shapes = Map::new();
    let mut enums = Map::new();
    let mut unions = Map::new();

    for field in &fields {
        let child = child_path(path, field);
        if (model.probe)(&set_path(root, &child, Value::Null)) {
            nullable.push(Value::String(field.clone()));
        }

        match &obj[field] {
            Value::Array(items) => {
                types.insert(field.clone(), json!("array"));
                let elem = format!("{child}[]");
                if items.is_empty() {
                    // 数组为空 ⇒ 推不出元素形状，而这会让这条闸门对它的内容
                    // 完全失明。丰满样本必须让每个数组至少有一个元素。
                    panic!("路径 `{child}` 是空数组，推不出元素形状 —— 丰满样本没写满");
                }
                if let Some(variants) = find_union(model, &elem) {
                    unions.insert(field.clone(), derive_union(root, &elem, variants, model));
                } else {
                    shapes.insert(field.clone(), derive_object(root, &elem, model));
                }
            }
            Value::Object(map) => {
                types.insert(field.clone(), json!("object"));
                if let Some(variants) = find_union(model, &child) {
                    unions.insert(field.clone(), derive_union(root, &child, variants, model));
                } else {
                    // 兜底：看着像 tagged union 却没登记 ⇒ 它的 props 会静默失管。
                    assert!(
                        !map.contains_key(UNION_TAG),
                        "路径 `{child}` 含 `{UNION_TAG}` 键，看着像 tagged union 却没有登记 —— \
                         去 `unions` 里登记它的每个变体，否则这个联合的字段集合不受管"
                    );
                    shapes.insert(field.clone(), derive_object(root, &child, model));
                }
            }
            Value::Null => {
                // 丰满样本里出现 null ⇒ 这个字段**非空时是什么**推不出来，
                // 于是闸门对它只知道「可空」、不知道「不空时是什么」——
                // 等于只对一半的事实有判据。这跟空数组是同一类失明，
                // 早期发现比让 contract 悄悄少一半信息好。
                panic!(
                    "路径 `{child}` 在丰满样本里是 null，推不出它的类型 —— \
                     把该 `Option` 填成 `Some(..)` 再生成（可空性由探测得出，不靠样本是 null）"
                );
            }
            other => {
                types.insert(field.clone(), json!(json_type(other)));
            }
        }

        if let Some(values) = find_enum(model, &child) {
            enums.insert(field.clone(), Value::Array(sorted(values)));
        }
    }

    json!({
        "fields": fields,
        "types": types,
        "nullable": nullable,
        "shapes": shapes,
        "enums": enums,
        "unions": unions,
    })
}

/// 推导一个 tagged union：逐个变体**注入**回文档，再按普通对象推导。
///
/// 注入而不是就地推导，是为了让变体内部字段的可空性也能被探测 ——
/// 探测需要一份完整的文档，孤立的变体片段自己永远能反序列化。
fn derive_union(root: &Value, path: &str, variants: &Variants, model: &Model) -> Value {
    let mut map = Map::new();
    for (name, variant) in variants {
        let injected = set_path(root, path, variant());
        map.insert((*name).to_string(), derive_object(&injected, path, model));
    }
    json!({ "tag": UNION_TAG, "variants": Value::Object(map) })
}

// ---------------------------------------------------------------------------
// 受管模型
// ---------------------------------------------------------------------------

/// tagged union 的变体清单：变体名 -> 该变体的样本构造器。
///
/// 延迟构造（`fn()` 而不是 `Value`）是因为变体样本要被**注入**回文档才能探测
/// 它内部字段的可空性，而注入需要对每个变体重新构造一份。
type Variants = Vec<(&'static str, fn() -> Value)>;

struct Sample {
    name: &'static str,
    /// 该样本里 union 的 tag。**没有** union 的模型留 `None`。
    expect_tag: Option<&'static str>,
    value: fn() -> Value,
}

struct Model {
    /// `contracts/` 下的文件名。
    file: &'static str,
    /// Rust 侧的类型名（写进 contract，出错时提示去哪改）。
    rust_type: &'static str,
    /// TypeScript 侧的类型所在文件。
    ts_type: &'static str,
    /// 丰满样本。**每个 `Option` 都取 `Some`、每个数组至少 1 个元素** ——
    /// 可空性与元素形状都从它推导，写不满就会让闸门失明。
    full: fn() -> Value,
    /// 能否反序列化。用它判定某个路径可不可为 `null`。
    probe: fn(&Value) -> bool,
    /// 路径 -> 该 unit enum 的**全部**线上取值。
    ///
    /// serde 没有运行时反射，枚举的全部取值只能手写。这是本机制**唯一的**
    /// 手抄点之一：新增一个变体而这里没跟上，contract 不会变。
    /// 缓解手段是下面 unions 那条断言与 TS 侧的比对。
    enums: Vec<(&'static str, Vec<Value>)>,
    /// 路径 -> tagged union 的**每个变体**（变体名 + 该变体的样本）。
    ///
    /// 同样只能手写变体清单；但**变体内部的字段**是推导出来的，
    /// 所以这里漏一个变体只会让那个变体失管，不会让整个字段失管。
    unions: Vec<(&'static str, Variants)>,
    /// 除形状外还受管的常量（例如读取上限）。没有则留 `Value::Null`。
    constants: Value,
    /// 样本里 union 的 tag 落在哪个路径。供 TS 侧断言
    /// 「解析出来的 tag 与 Rust 声明的 tag 一致」。
    union_tag_path: Option<&'static str>,
    /// 进 contract 的样本。每个 union 变体**至少**要有一个样本，
    /// 否则逐样本的解析用例会漏掉某一类。
    samples: Vec<Sample>,
}

fn serialize<T: serde::Serialize>(value: T) -> Value {
    to_value(value).expect("serialize")
}

fn unit_variants<T: serde::Serialize>(variants: &[T]) -> Vec<Value> {
    variants.iter().map(serialize).collect()
}

// -- RepoSnapshot ----------------------------------------------------------

fn snapshot_full() -> Value {
    serialize(RepoSnapshot {
        path: PathBuf::from("/repo"),
        head: HeadInfo {
            branch: Some("main".to_string()),
            commit: "1a2b3c4".to_string(),
            detached: false,
            upstream: Some(UpstreamInfo {
                name: "origin/main".to_string(),
                ahead: 1,
                behind: 2,
            }),
        },
        branches: vec![BranchInfo {
            name: "main".to_string(),
            commit: "1a2b3c4".to_string(),
            is_current: true,
        }],
        tags: vec![TagInfo {
            name: "v0.1.0".to_string(),
            commit: "1a2b3c4".to_string(),
        }],
        status: StatusInfo {
            conflicts: vec![FileChange {
                path: "conflicted.txt".to_string(),
                kind: ChangeKind::Unmerged,
            }],
            staged: vec![FileChange {
                path: "staged.txt".to_string(),
                kind: ChangeKind::Added,
            }],
            unstaged: vec![FileChange {
                path: "unstaged.txt".to_string(),
                kind: ChangeKind::Modified,
            }],
        },
        state: Some(RepoState::Rebase {
            step: Some(3),
            total: Some(7),
        }),
    })
}

fn snapshot_probe(value: &Value) -> bool {
    from_value::<RepoSnapshot>(value.clone()).is_ok()
}

fn change_kinds() -> Vec<Value> {
    unit_variants(&[
        ChangeKind::Added,
        ChangeKind::Modified,
        ChangeKind::Deleted,
        ChangeKind::Renamed,
        ChangeKind::Copied,
        ChangeKind::TypeChanged,
        ChangeKind::Unmerged,
        ChangeKind::Unknown,
    ])
}

fn repo_state_variants() -> Variants {
    vec![
        ("merge", (|| serialize(RepoState::Merge)) as fn() -> Value),
        (
            "rebase",
            (|| {
                serialize(RepoState::Rebase {
                    step: Some(3),
                    total: Some(7),
                })
            }) as fn() -> Value,
        ),
        (
            "cherry_pick",
            (|| serialize(RepoState::CherryPick)) as fn() -> Value,
        ),
        ("revert", (|| serialize(RepoState::Revert)) as fn() -> Value),
        ("bisect", (|| serialize(RepoState::Bisect)) as fn() -> Value),
    ]
}

fn snapshot_with_state(state: Option<RepoState>) -> Value {
    let mut base = snapshot_full();
    base["state"] = to_value(state).expect("serialize state");
    base
}

fn snapshot_model() -> Model {
    Model {
        file: "snapshot.json",
        rust_type: "repo_prism_core::RepoSnapshot",
        ts_type: "src/lib/snapshot.ts",
        full: snapshot_full,
        probe: snapshot_probe,
        enums: vec![
            ("status.conflicts[].kind", change_kinds()),
            ("status.staged[].kind", change_kinds()),
            ("status.unstaged[].kind", change_kinds()),
        ],
        unions: vec![("state", repo_state_variants())],
        constants: Value::Null,
        union_tag_path: Some("state.kind"),
        samples: vec![
            Sample {
                name: "full",
                expect_tag: Some("rebase"),
                value: snapshot_full,
            },
            Sample {
                name: "clean",
                expect_tag: None,
                value: || snapshot_with_state(None),
            },
            Sample {
                name: "merge",
                expect_tag: Some("merge"),
                value: || snapshot_with_state(Some(RepoState::Merge)),
            },
            Sample {
                name: "cherry_pick",
                expect_tag: Some("cherry_pick"),
                value: || snapshot_with_state(Some(RepoState::CherryPick)),
            },
            Sample {
                name: "revert",
                expect_tag: Some("revert"),
                value: || snapshot_with_state(Some(RepoState::Revert)),
            },
            Sample {
                name: "bisect",
                expect_tag: Some("bisect"),
                value: || snapshot_with_state(Some(RepoState::Bisect)),
            },
            Sample {
                name: "rebase_without_progress",
                expect_tag: Some("rebase"),
                value: || {
                    snapshot_with_state(Some(RepoState::Rebase {
                        step: None,
                        total: None,
                    }))
                },
            },
            Sample {
                name: "detached",
                expect_tag: None,
                value: || {
                    let mut base = snapshot_with_state(None);
                    base["head"]["branch"] = Value::Null;
                    base["head"]["detached"] = json!(true);
                    base["head"]["upstream"] = Value::Null;
                    base
                },
            },
        ],
    }
}

// -- WorkspaceInfo ---------------------------------------------------------

fn workspace_full() -> Value {
    serialize(WorkspaceInfo {
        worktrees: vec![WorktreeInfo {
            path: PathBuf::from("/repo"),
            branch: Some("main".to_string()),
            commit: "1a2b3c4".to_string(),
            is_main: true,
            bare: false,
            detached: false,
            locked: false,
        }],
        stashes: vec![StashInfo {
            reference: "stash@{0}".to_string(),
            commit: "5d6e7f8".to_string(),
            message: "WIP on main: 1a2b3c4 subject".to_string(),
        }],
    })
}

fn workspace_model() -> Model {
    Model {
        file: "workspace.json",
        rust_type: "repo_prism_core::WorkspaceInfo",
        ts_type: "src/lib/workspaceInfo.ts",
        full: workspace_full,
        probe: |value| from_value::<WorkspaceInfo>(value.clone()).is_ok(),
        enums: vec![],
        unions: vec![],
        constants: Value::Null,
        union_tag_path: None,
        samples: vec![
            Sample {
                name: "full",
                expect_tag: None,
                value: workspace_full,
            },
            Sample {
                name: "empty",
                expect_tag: None,
                value: || serialize(WorkspaceInfo::default()),
            },
            Sample {
                name: "detached_worktree",
                expect_tag: None,
                value: || {
                    let mut base = workspace_full();
                    base["worktrees"][0]["branch"] = Value::Null;
                    base["worktrees"][0]["detached"] = json!(true);
                    base
                },
            },
        ],
    }
}

// -- ChangeAnalysis --------------------------------------------------------

fn analysis_full() -> Value {
    serialize(ChangeAnalysis {
        summary: "3 个文件有未提交改动".to_string(),
        total_files: 3,
        risks: vec![Risk {
            rule_id: "large-deletion".to_string(),
            level: RiskLevel::Critical,
            message: "删除了 420 行".to_string(),
            path: "src/legacy.rs".to_string(),
        }],
        by_level: RiskCounts {
            info: 1,
            warn: 2,
            critical: 3,
        },
    })
}

fn analysis_model() -> Model {
    Model {
        file: "analysis.json",
        rust_type: "repo_prism_core::ChangeAnalysis",
        ts_type: "src/lib/analysis.ts",
        full: analysis_full,
        probe: |value| from_value::<ChangeAnalysis>(value.clone()).is_ok(),
        enums: vec![(
            "risks[].level",
            unit_variants(&[RiskLevel::Info, RiskLevel::Warn, RiskLevel::Critical]),
        )],
        unions: vec![],
        constants: Value::Null,
        union_tag_path: None,
        samples: vec![
            Sample {
                name: "full",
                expect_tag: None,
                value: analysis_full,
            },
            Sample {
                name: "empty",
                expect_tag: None,
                value: || {
                    serialize(ChangeAnalysis {
                        summary: String::new(),
                        total_files: 0,
                        risks: vec![],
                        by_level: RiskCounts::default(),
                    })
                },
            },
        ],
    }
}

// -- BlobPreview（P-12 的模型，P-13 迁到通用推导器） ------------------------

fn preview_base(kind: BlobKind, size: u64) -> BlobPreview {
    BlobPreview {
        size,
        kind,
        content: None,
        text: None,
        hex: None,
        truncated: false,
        too_large: false,
    }
}

fn blob_variants() -> Variants {
    vec![
        (
            "image",
            (|| {
                serialize(BlobKind::Image {
                    format: ImageFormat::Png,
                })
            }) as fn() -> Value,
        ),
        ("text", (|| serialize(BlobKind::Text)) as fn() -> Value),
        ("binary", (|| serialize(BlobKind::Binary)) as fn() -> Value),
        (
            "lfs_pointer",
            (|| {
                serialize(BlobKind::LfsPointer {
                    oid: "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393"
                        .to_string(),
                    size: 12_345,
                })
            }) as fn() -> Value,
        ),
        (
            "too_large",
            (|| serialize(BlobKind::TooLarge)) as fn() -> Value,
        ),
    ]
}

/// 形状探针：**每个 `Option` 都填 `Some`**。
///
/// 它不表示「一个真实的预览」—— 真实的预览每类只填该填的那一项，那些是
/// `samples` 里的事。这里三个都填，是为了让三个字段的**非空类型**都能被推出来
/// （可空性另由探测得出，不靠这里是 null）。
fn blob_full() -> Value {
    serialize(BlobPreview {
        size: 11,
        kind: BlobKind::Text,
        content: Some("Y29udGVudA==".to_string()),
        text: Some("hello blob\n".to_string()),
        hex: Some("00000000  68 65 6c 6c 6f".to_string()),
        truncated: false,
        too_large: false,
    })
}

fn blob_model() -> Model {
    Model {
        file: "blob-preview.json",
        rust_type: "repo_prism_core::BlobPreview",
        ts_type: "src/lib/blobPreview.ts",
        full: blob_full,
        probe: |value| from_value::<BlobPreview>(value.clone()).is_ok(),
        enums: vec![(
            "kind.format",
            unit_variants(&[
                ImageFormat::Png,
                ImageFormat::Jpeg,
                ImageFormat::Gif,
                ImageFormat::Webp,
                ImageFormat::Bmp,
                ImageFormat::Svg,
            ]),
        )],
        unions: vec![("kind", blob_variants())],
        constants: json!({ "max_preview_bytes": MAX_PREVIEW_BYTES }),
        union_tag_path: Some("kind.kind"),
        samples: vec![
            Sample {
                name: "image/png",
                expect_tag: Some("image"),
                value: || {
                    serialize(BlobPreview {
                        content: Some("iVBORw0KGgo=".to_string()),
                        ..preview_base(
                            BlobKind::Image {
                                format: ImageFormat::Png,
                            },
                            137,
                        )
                    })
                },
            },
            Sample {
                name: "image/svg",
                expect_tag: Some("image"),
                value: || {
                    serialize(BlobPreview {
                        content: Some("PHN2Zy8+".to_string()),
                        ..preview_base(
                            BlobKind::Image {
                                format: ImageFormat::Svg,
                            },
                            9,
                        )
                    })
                },
            },
            Sample {
                name: "text",
                expect_tag: Some("text"),
                value: || {
                    serialize(BlobPreview {
                        text: Some("hello blob\n".to_string()),
                        ..preview_base(BlobKind::Text, 11)
                    })
                },
            },
            Sample {
                name: "text/truncated",
                expect_tag: Some("text"),
                value: || {
                    serialize(BlobPreview {
                        text: Some("前一部分\n".to_string()),
                        truncated: true,
                        ..preview_base(BlobKind::Text, 9_999)
                    })
                },
            },
            Sample {
                name: "binary",
                expect_tag: Some("binary"),
                value: || {
                    serialize(BlobPreview {
                        hex: Some("00000000  68 65 6c 6c 6f".to_string()),
                        ..preview_base(BlobKind::Binary, 5)
                    })
                },
            },
            Sample {
                name: "lfs_pointer",
                expect_tag: Some("lfs_pointer"),
                value: || {
                    serialize(BlobPreview {
                        // 外层 size 是**指针文件**的大小，kind.size 是它指向对象的大小。
                        // 两者同名不同义 —— 故意钉在样本里，别指望相等。
                        size: 130,
                        kind: BlobKind::LfsPointer {
                            oid: "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393"
                                .to_string(),
                            size: 12_345,
                        },
                        ..preview_base(BlobKind::Text, 0)
                    })
                },
            },
            Sample {
                name: "too_large",
                expect_tag: Some("too_large"),
                value: || {
                    serialize(BlobPreview {
                        size: MAX_PREVIEW_BYTES + 1,
                        too_large: true,
                        ..preview_base(BlobKind::TooLarge, MAX_PREVIEW_BYTES + 1)
                    })
                },
            },
        ],
    }
}

fn models() -> Vec<Model> {
    vec![
        snapshot_model(),
        workspace_model(),
        analysis_model(),
        blob_model(),
    ]
}

// ---------------------------------------------------------------------------
// 生成与比对
// ---------------------------------------------------------------------------

fn contract_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts")
}

fn build_contract(model: &Model) -> String {
    let full = (model.full)();
    assert!(
        (model.probe)(&full),
        "`{}` 的丰满样本自己都反序列化不了 —— 形状推导会从这里开始全错",
        model.rust_type
    );

    let mut root = Map::new();
    root.insert(
        "_".to_string(),
        json!({
            "readme": "本文件由 cargo test -p repo-prism-core --test wire_contract -- --ignored 生成，请勿手改。它是 Rust 的 serde 形状与 TypeScript 侧类型之间的唯一事实来源。",
            "rust_type": model.rust_type,
            "ts_type": model.ts_type,
        }),
    );
    root.insert("shape".to_string(), derive_object(&full, "", model));
    if let Some(path) = model.union_tag_path {
        root.insert("union_tag_path".to_string(), json!(path));
    }
    if !model.constants.is_null() {
        root.insert("constants".to_string(), model.constants.clone());
    }

    let samples: Vec<Value> = model
        .samples
        .iter()
        .map(|sample| {
            let mut entry = Map::new();
            entry.insert("name".to_string(), json!(sample.name));
            if let Some(tag) = sample.expect_tag {
                entry.insert("expect_tag".to_string(), json!(tag));
            }
            entry.insert("value".to_string(), (sample.value)());
            Value::Object(entry)
        })
        .collect();
    root.insert("samples".to_string(), Value::Array(samples));

    let mut text = serde_json::to_string_pretty(&Value::Object(root)).expect("serialize contract");
    text.push('\n');
    text
}

/// 抹掉行尾差异再比对。
///
/// Windows runner 的默认 `autocrlf` 会在 checkout 时把 LF 转成 CRLF，
/// 而 `serde_json` 生成的永远是 LF —— 逐字节比对会红，还会假装是「形状变了」，
/// 把人引到完全错误的方向上去（**P-12 在 `0705e46` 上真实发生过**，
/// macos / ubuntu 全绿、只有 windows 那条腿红）。
///
/// 根由由仓库根的 `.gitattributes` 治（`contracts/** text eol=lf`），
/// 这里是第二道防线：不是所有环境都读那份配置（子模块、归档解包、`git archive`…）。
fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// 前几处不同的行。
///
/// 整份 JSON 打进断言消息有几千字符，而人真正要找的是「哪一行不一样」——
/// 直接 diff 全文会把人淹没在噪声里（P-12 那次还因此把人引向了错误的方向）。
///
/// 取**三处**而不是一处：差异常常是「一个数组少了一项」，那时第一处不同的行
/// 只是 `"nullable": [` 这种容器开头，真正的差别在下面几行 —— 只报一行会把人
/// 卡在「两边看起来一样啊」（探针 P2 就是这样）。
fn first_differences(expected: &str, actual: &str) -> Option<String> {
    const LIMIT: usize = 3;
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let shared = expected_lines.len().min(actual_lines.len());

    let mut found: Vec<String> = Vec::new();
    for (index, (want, got)) in expected_lines[..shared]
        .iter()
        .zip(actual_lines[..shared].iter())
        .enumerate()
    {
        if want != got {
            found.push(format!(
                "  第 {} 行：contract 里是 {got} / 当前形状是 {want}",
                index + 1
            ));
            if found.len() >= LIMIT {
                break;
            }
        }
    }
    if found.is_empty() {
        if expected_lines.len() == actual_lines.len() {
            return None;
        }
        found.push(format!(
            "  行数不同：contract 里 {} 行，当前形状 {} 行（前 {} 行都一样）",
            actual_lines.len(),
            expected_lines.len(),
            shared
        ));
    }
    Some(found.join("\n"))
}

#[test]
fn every_contract_file_matches_the_serde_shape() {
    let dir = contract_dir();
    for model in models() {
        let path = dir.join(model.file);
        let expected = build_contract(&model);
        let actual = fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("读不到 contract 文件 {path:?}：{e}。若它是新增的，先跑一次 --ignored 生成它")
        });

        let normalized = normalize_newlines(&actual);
        if normalized != expected {
            let diff = first_differences(&expected, &normalized)
                .unwrap_or_else(|| "  （逐行看不出差别 —— 检查行尾与不可见字符）".to_string());
            panic!(
                "\n\ncontract 文件 {path:?} 与 serde 的当前形状不一致。\n{diff}\n\n\
                 这通常意味着你改了 `{}` 的序列化（字段名、rename_all、新增或删除字段、\n\
                 `String` 改成 `Option<String>`），但还没有把新形状同步到前端。\n\n\
                 如果改动是有意的，跑这条更新并**同时**改 TypeScript 侧：\n\n    \
                 cargo test -p repo-prism-core --test wire_contract -- --ignored\n\n\
                 注意 TypeScript 侧的测试不会因为这条更新而自动通过 ——\n\
                 它读的是同一个文件，所以它会告诉你 TS 那份键表要跟着改什么。\n",
                model.rust_type
            );
        }
    }
}

/// 重新生成全部 contract 文件。**默认不跑**（`#[ignore]`）—— 形状变了必须由人
/// 明确执行，CI 不会替你更新。
#[test]
#[ignore = "形状变化必须是人有意的；跑这条会重写 contracts/ 下的全部文件"]
fn regenerate_the_contract_files() {
    let dir = contract_dir();
    for model in models() {
        let path = dir.join(model.file);
        fs::write(&path, build_contract(&model)).expect("write contract");
        println!("已重新生成 {path:?}");
    }
    println!("接下来记得同步 TypeScript 侧的键表，并跑 pnpm test");
}
