//! 跨语言线格式 contract（`BlobPreview`）。
//!
//! # 为什么要有这个文件
//!
//! `BlobPreview` 的 JSON 形状是**三处**消费者的共同契约：桌面端 UI、CLI、MCP。
//! Rust 侧的形状由 serde 的 derive 决定，而前端 `src/lib/blobPreview.ts` 里那份
//! 类型是**手抄**的 —— 两边没有任何编译期联系。
//!
//! 于是存在一类会导致「Rust 测试全绿、UI 静默变空白」的改动：改一个
//! `rename_all`、改一个字段名、加一个字段。单测测的是 Rust 结构，
//! 而跨语言消费者拿到的是 JSON —— 这两者此前没有任何东西把它们连起来。
//!
//! # 怎么连起来
//!
//! `contracts/blob-preview.json` 是唯一事实来源，两侧都去对它：
//!
//! ```text
//!   serde derive  ──生成──▶  contracts/blob-preview.json  ◀──读──  src/lib/blobPreview.test.ts
//!        │                            ▲                                      │
//!        └── 改了 ⟹ 本文件红 ────────┘                                      └── TS 清单改了 ⟹ 那边红
//! ```
//!
//! - Rust 改了形状 ⇒ **这里**红，提示去重新生成
//! - TS 改了清单 / 类型 ⇒ `src/lib/blobPreview.test.ts` 红
//! - 两边同时改但不一致 ⇒ 两侧都红
//!
//! # 怎么更新
//!
//! ```bash
//! cargo test -p repo-prism-core --test wire_contract -- --ignored
//! ```
//!
//! 那条 `--ignored` 的用例会重写文件。**它是被 ignore 的，所以 CI 不会替你更新**：
//! 形状变了必须由人明确执行这条命令，并把 contract 文件的改动连同两侧适配
//! 一起提交。这跟 `tests/scale.rs` 的诊断基准是同一个约定。

use repo_prism_core::{BlobKind, BlobPreview, ImageFormat, MAX_PREVIEW_BYTES};
use serde_json::{json, to_value, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// contract 文件的位置。放在仓库根的 `contracts/`，两侧都能读到。
fn contract_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts")
        .join("blob-preview.json")
}

/// 一个对象在 JSON 里的字段名，**排序后**返回。
///
/// 排序是必要的：`Value::Object` 是 `BTreeMap`（字母序），而结构体序列化
/// 的声明顺序与之不同。不排序的话比对会在两边都稳定但互不相同。
fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("JSON 对象")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn preview(kind: BlobKind, size: u64) -> BlobPreview {
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

/// contract 里的一个样本：一段真实序列化出来的 JSON，外加它应该被
/// TypeScript 侧解析成的 tag。
fn sample(name: &str, expect_tag: &str, preview: BlobPreview) -> Value {
    json!({
        "name": name,
        "expect_tag": expect_tag,
        "value": to_value(&preview).expect("serialize preview"),
    })
}

/// `ImageFormat` 在线上的全部取值。新增一种图片格式时，TS 侧的
/// `IMAGE_FORMATS` 会先红 —— 这正是这条清单存在的理由。
fn image_formats() -> Vec<String> {
    [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Gif,
        ImageFormat::Webp,
        ImageFormat::Bmp,
        ImageFormat::Svg,
    ]
    .iter()
    .map(|f| {
        to_value(f)
            .expect("serialize format")
            .as_str()
            .expect("unit variant serializes to a string")
            .to_string()
    })
    .collect()
}

/// 每个 kind 在线上的字段清单（含 `kind` 这个 tag 本身）。
fn kind_shapes() -> Map<String, Value> {
    let variants = [
        BlobKind::Image {
            format: ImageFormat::Png,
        },
        BlobKind::Text,
        BlobKind::Binary,
        BlobKind::LfsPointer {
            oid: "4d7a2146".to_string(),
            size: 12_345,
        },
        BlobKind::TooLarge,
    ];

    let mut map = Map::new();
    for kind in variants {
        let value = to_value(&kind).expect("serialize kind");
        let tag = value["kind"]
            .as_str()
            .expect("internally tagged")
            .to_string();
        map.insert(tag, json!({ "props": sorted_keys(&value) }));
    }
    map
}

fn build_contract() -> String {
    let shape_holder = preview(BlobKind::Text, 11);
    let fields = sorted_keys(&to_value(&shape_holder).expect("serialize preview"));

    let samples = vec![
        sample(
            "image/png",
            "image",
            BlobPreview {
                content: Some("iVBORw0KGgo=".to_string()),
                ..preview(
                    BlobKind::Image {
                        format: ImageFormat::Png,
                    },
                    137,
                )
            },
        ),
        sample(
            "image/svg",
            "image",
            BlobPreview {
                content: Some("PHN2Zy8+".to_string()),
                ..preview(
                    BlobKind::Image {
                        format: ImageFormat::Svg,
                    },
                    9,
                )
            },
        ),
        sample(
            "text",
            "text",
            BlobPreview {
                text: Some("hello blob\n".to_string()),
                ..preview(BlobKind::Text, 11)
            },
        ),
        sample(
            "text/truncated",
            "text",
            BlobPreview {
                text: Some("前一部分\n".to_string()),
                truncated: true,
                ..preview(BlobKind::Text, 9_999)
            },
        ),
        sample(
            "binary",
            "binary",
            BlobPreview {
                hex: Some("00000000  68 65 6c 6c 6f".to_string()),
                ..preview(BlobKind::Binary, 5)
            },
        ),
        sample(
            "lfs_pointer",
            "lfs_pointer",
            BlobPreview {
                // 外层 size 是**指针文件**的大小，kind.size 是它指向的对象的大小。
                // 两者同名不同义 —— 这是故意钉在样本里的，别指望相等。
                size: 130,
                kind: BlobKind::LfsPointer {
                    oid: "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393"
                        .to_string(),
                    size: 12_345,
                },
                ..preview(BlobKind::Text, 0)
            },
        ),
        sample(
            "too_large",
            "too_large",
            BlobPreview {
                size: MAX_PREVIEW_BYTES + 1,
                too_large: true,
                ..preview(BlobKind::TooLarge, MAX_PREVIEW_BYTES + 1)
            },
        ),
    ];

    let contract = json!({
        "_": {
            "readme": "本文件由 cargo test -p repo-prism-core --test wire_contract -- --ignored 生成，请勿手改。它是 Rust 的 serde 形状与 TypeScript 侧类型之间的唯一事实来源。",
            "rust_type": "repo_prism_core::BlobPreview",
            "ts_type": "src/lib/blobPreview.ts",
        },
        "fields": fields,
        "kinds": kind_shapes(),
        "image_formats": image_formats(),
        "max_preview_bytes": MAX_PREVIEW_BYTES,
        "samples": samples,
    });

    let mut text = serde_json::to_string_pretty(&contract).expect("serialize contract");
    text.push('\n');
    text
}

#[test]
fn the_wire_contract_file_matches_the_serde_shape() {
    let path = contract_path();
    let expected = build_contract();
    let actual = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("读不到 contract 文件 {path:?}：{e}。若它是新增的，先跑一次 --ignored 生成它")
    });

    assert_eq!(
        actual, expected,
        "\n\ncontract 文件与 serde 的当前形状不一致。\n\
         这通常意味着你改了 `BlobPreview` / `BlobKind` 的序列化（字段名、rename_all、\n\
         新增或删除字段），但还没有把新形状同步到前端。\n\n\
         如果改动是有意的，跑这条更新并**同时**改 `src/lib/blobPreview.ts`：\n\n    \
         cargo test -p repo-prism-core --test wire_contract -- --ignored\n\n\
         注意 TypeScript 侧的测试不会因为这条更新而自动通过 ——\n\
         它读的是同一个文件，所以它会告诉你 TS 那份清单要跟着改什么。\n"
    );
}

/// 重新生成 contract 文件。**默认不跑**（`#[ignore]`）—— 形状变了必须由人明确
/// 执行，CI 不会替你更新。
#[test]
#[ignore = "形状变化必须是人有意的；跑这条会重写 contracts/blob-preview.json"]
fn regenerate_the_wire_contract_file() {
    let path = contract_path();
    fs::write(&path, build_contract()).expect("write contract");
    println!("已重新生成 {path:?}");
    println!("接下来记得同步 `src/lib/blobPreview.ts` 里的清单，并跑 pnpm test");
}
