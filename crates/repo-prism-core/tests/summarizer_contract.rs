//! 与 Ollama **官方 API 文档**的契约核对。
//!
//! # 它和 `summarizer_http.rs` 的区别（很重要）
//!
//! `summarizer_http.rs` 的 stub 响应体是**我们自己写的**。这带来一个无法回避的盲区：
//! 如果 `summarizer.rs` 里把字段名写错（`message` 写成 `response`），
//! 我们大概率也会照着同一个错误去写 stub 的响应——两边一起错，测试照样绿。
//! P-07 的文件头把这一点写得很清楚：stub 证明不了「字段名与真实响应一致」。
//!
//! 本文件用**厂商自己公布的响应示例**做输入，于是这个盲区被拿掉：
//! 字段名不再由我们定义，而是由被对接方定义。若 `summarizer.rs` 哪天把
//! `response` 改成别的名字，`generate_reads_the_vendor_published_...` 会红。
//!
//! # 来源
//!
//! 以下 payload 逐字取自 Ollama 官方仓库的 API 文档 `docs/api.md`
//! （<https://github.com/ollama/ollama/blob/main/docs/api.md>，取回日期 2026-10-09）：
//!
//! - `VENDOR_TAGS_PAYLOAD` ← 该文档 “List Local Models” 一节的 Response 示例
//! - `VENDOR_GENERATE_PAYLOAD` ← “Generate a completion” 一节
//!   “If `stream` is set to `false`, the response will be a single JSON object” 的示例
//!
//! 刻意**逐字保留**全部字段（`context`、`total_duration`、`eval_count`、
//! 嵌套的 `details`、纳秒精度的 `modified_at`…），其中大部分我们用不到。
//! 「用不到」恰恰是要测的：真实响应必然带一堆我们不认识的字段，
//! 解析器必须对它们免疫，而不是靠字段数量刚好对上。
//!
//! # 仍然不是端到端验证
//!
//! 输入来自文档而不是一台真的 Ollama，所以它证明的是「我们读得懂厂商公布的形状」。
//! 文档与某个具体版本的实现之间仍有差距；真正跑在真实模型上的端到端验证
//! 在 SPEC US-7 里依旧是 `[待实现]`。

mod common;

use common::stub::Stub;
use repo_prism_core::{OllamaConfig, OllamaSummarizer, DEFAULT_MODEL};

/// 逐字取自 Ollama `docs/api.md` 的 “List Local Models” 响应示例。
const VENDOR_TAGS_PAYLOAD: &str = r#"{
  "models": [
    {
      "name": "deepseek-r1:latest",
      "model": "deepseek-r1:latest",
      "modified_at": "2025-05-10T08:06:48.639712648-07:00",
      "size": 4683075271,
      "digest": "0a8c266910232fd3291e71e5ba1e058cc5af9d411192cf88b6d30e92b6e73163",
      "details": {
        "parent_model": "",
        "format": "gguf",
        "family": "qwen2",
        "families": ["qwen2"],
        "parameter_size": "7.6B",
        "quantization_level": "Q4_K_M"
      }
    },
    {
      "name": "llama3.2:latest",
      "model": "llama3.2:latest",
      "modified_at": "2025-05-04T17:37:44.706015396-07:00",
      "size": 2019393189,
      "digest": "a80c4f17acd55265feec403c7aef86be0c25983ab279d83f3bcd3abbcb5b8b72",
      "details": {
        "parent_model": "",
        "format": "gguf",
        "family": "llama",
        "families": ["llama"],
        "parameter_size": "3.2B",
        "quantization_level": "Q4_K_M"
      }
    }
  ]
}"#;

/// 逐字取自 Ollama `docs/api.md` 的非流式生成响应示例。
const VENDOR_GENERATE_PAYLOAD: &str = r#"{
  "model": "llama3.2",
  "created_at": "2023-08-04T19:22:45.499127Z",
  "response": "The sky is blue because it is the color of the sky.",
  "done": true,
  "context": [1, 2, 3],
  "total_duration": 5043500667,
  "load_duration": 5025959,
  "prompt_eval_count": 26,
  "prompt_eval_duration": 325953000,
  "eval_count": 290,
  "eval_duration": 4709213000
}"#;

fn summarizer_for(endpoint: &str) -> OllamaSummarizer {
    OllamaSummarizer::new(OllamaConfig {
        endpoint: endpoint.to_string(),
        model: DEFAULT_MODEL.to_string(),
        timeout_secs: 30,
    })
    .expect("回环地址上的合法配置必须构造成功")
}

#[test]
fn list_models_reads_the_vendor_published_tags_payload() {
    let stub = Stub::start(1, |_| (200, VENDOR_TAGS_PAYLOAD.to_string()));

    let models = summarizer_for(&stub.endpoint())
        .list_models()
        .expect("厂商公布的 payload 必须能被解析");

    assert_eq!(
        models,
        vec!["deepseek-r1:latest", "llama3.2:latest"],
        "应取到全部模型名，且保持响应中的顺序"
    );
    stub.finish();
}

#[test]
fn generate_reads_the_vendor_published_non_streaming_payload() {
    let stub = Stub::start(1, |_| (200, VENDOR_GENERATE_PAYLOAD.to_string()));

    let text = summarizer_for(&stub.endpoint()).generate("Why is the sky blue?");

    assert_eq!(
        text.as_deref(),
        Some("The sky is blue because it is the color of the sky."),
        "应从 response 字段取出正文；取不到就说明字段名与厂商不一致"
    );
    stub.finish();
}

/// 反向用例：**证明上面那条断言真的在核对字段名**。
///
/// 把 `response` 换成别的名字（模拟「有人改错了字段名，或者上游改了 API」），
/// 解析必须失败而不是当成「模型没说话」。没有这条，
/// `generate_reads_the_vendor_published_non_streaming_payload` 只能证明
/// 「我们的解析器能读出我们自己写的东西」——那就退回到 P-07 的盲区里了。
#[test]
fn a_renamed_response_field_is_not_silently_accepted() {
    let stub = Stub::start(1, |_| {
        (
            200,
            r#"{"model":"llama3.2","message":"这个字段名不是 Ollama 的","done":true}"#.to_string(),
        )
    });

    let text = summarizer_for(&stub.endpoint()).generate("hi");

    assert!(
        text.is_none(),
        "字段名不是 response 时必须取不到正文（降级为 None），实际拿到：{text:?}"
    );
    stub.finish();
}

/// 流式响应是**多行 NDJSON**（每行一个 JSON 对象）。
///
/// 我们请求时钉死了 `stream: false`（`summarizer_http.rs` 有断言），
/// 这条用例说明那个钉子的**后果**：万一 stream 没关，服务端回的是多行 NDJSON，
/// `into_json()` 会失败并降级为 None。
///
/// 这比「悄悄返回第一段碎片」好得多——只截到前几个 token 的摘要
/// 看起来像模型在胡言乱语，而且不会有任何错误冒出来。
#[test]
fn an_ndjson_stream_is_refused_rather_than_truncated() {
    let stub = Stub::start(1, |_| {
        (
            200,
            concat!(
                r#"{"model":"llama3.2","created_at":"2023-08-04T19:22:45.499127Z","response":"The","done":false}"#,
                "\n",
                r#"{"model":"llama3.2","created_at":"2023-08-04T19:22:45.499127Z","response":" sky","done":false}"#,
                "\n",
                r#"{"model":"llama3.2","created_at":"2023-08-04T19:22:45.499127Z","response":" is blue.","done":true}"#,
                "\n"
            )
            .to_string(),
        )
    });

    let text = summarizer_for(&stub.endpoint()).generate("Why is the sky blue?");

    assert!(
        text.is_none(),
        "多行 NDJSON 必须整体判为解析失败，不能只截取第一行当摘要；实际拿到：{text:?}"
    );
    stub.finish();
}
