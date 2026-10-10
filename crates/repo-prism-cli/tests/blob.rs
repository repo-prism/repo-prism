//! `repoprism blob` 集成测试（工程补丁 P-11）。
//!
//! 全部通过**真实子进程**执行打包好的二进制：CLI 的价值就在进程边界上。
//!
//! 这里最要紧的不是「读到的对不对」（core 侧 `tests/preview.rs` 已经钉住），
//! 而是**命令是否真的把 P-10 的边界带到了进程之外**：
//! `--size-only` 必须一个字节都不读，失败的三种情形必须报出来而不是降级。

mod common;

use common::TempRepo;
use std::process::Command;

fn repoprism() -> Command {
    Command::new(env!("CARGO_BIN_EXE_repoprism"))
}

fn json_of(out: &[u8]) -> serde_json::Value {
    serde_json::from_slice(out).expect("输出必须是合法 JSON")
}

/// PNG 的最小可识别前缀（8 字节魔法）。分类靠的是字节不是扩展名。
///
/// 必须是 `&[u8]` 而不是 `&str`：`'\u{89}'` 在 UTF-8 里是**两个字节**（0xC2 0x89），
/// 写成字符串字面量得到的前缀根本不是 PNG 的魔法（实测会被判成 text）。
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n not really a png";

/// 直接写字节。`TempRepo::write` 只接受 `&str`，图片与任意二进制走不到它。
fn write_bytes(repo: &TempRepo, rel: &str, bytes: &[u8]) {
    std::fs::write(repo.path().join(rel), bytes).expect("failed to write file");
}

#[test]
fn blob_json_returns_a_text_preview_in_the_shared_envelope() {
    let repo = TempRepo::new("blob-text");
    repo.write("a.txt", "hello blob\n");
    repo.commit("c1");

    let out = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "a.txt", "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(
        out.status.success(),
        "blob 应成功退出，stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    let value = json_of(&out.stdout);
    // 与其它三条数据命令共用同一套信封，Agent 只需实现一次解析
    assert_eq!(value["schema_version"], "1");
    assert_eq!(value["tool"], "repoprism");

    let data = &value["data"];
    assert_eq!(data["kind"]["kind"], "text");
    assert_eq!(data["text"], "hello blob\n");
    assert_eq!(data["size"], 11);
    assert_eq!(data["too_large"], false);
    assert_eq!(data["truncated"], false);
}

#[test]
fn blob_size_only_reports_the_size_and_reads_nothing() {
    let repo = TempRepo::new("blob-size");
    repo.write("a.txt", "hello blob\n");
    repo.commit("c1");

    let out = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "a.txt", "--size-only", "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(out.status.success());
    let data = json_of(&out.stdout)["data"].clone();
    assert_eq!(data["size"], 11);
    assert_eq!(data["preview_cap_bytes"], 4 * 1024 * 1024);
    // 「一个字节都没读」要有可断言的形状：内容字段**根本不存在**。
    for field in ["text", "content", "hex", "kind"] {
        assert!(
            data.get(field).is_none(),
            "--size-only 的结果里不应出现 {field}，实际 {data}"
        );
    }
}

#[test]
fn blob_defaults_to_head_and_reads_a_historic_revision_on_request() {
    let repo = TempRepo::new("blob-rev");
    repo.write("a.txt", "old\n");
    repo.commit("c1");
    repo.write("a.txt", "new\n");
    repo.commit("c2");

    let head = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "a.txt", "--json"])
        .output()
        .expect("failed to run repoprism");
    assert_eq!(json_of(&head.stdout)["data"]["text"], "new\n");

    let old = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "a.txt", "--rev", "HEAD~1", "--json"])
        .output()
        .expect("failed to run repoprism");
    assert_eq!(
        json_of(&old.stdout)["data"]["text"],
        "old\n",
        "--rev 必须真的按那个版本读"
    );
}

#[test]
fn blob_on_a_missing_file_fails_instead_of_returning_an_empty_preview() {
    let repo = TempRepo::new("blob-missing");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let out = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "nope.txt", "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(!out.status.success(), "文件不存在必须非零退出");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.is_empty(), "失败必须给出 stderr");
    assert!(
        stderr.contains("不存在"),
        "错误信息要说清是『不存在』，stderr={stderr}"
    );
}

#[test]
fn blob_on_a_directory_fails_even_though_the_size_query_succeeds() {
    let repo = TempRepo::new("blob-dir");
    std::fs::create_dir_all(repo.path().join("nested")).expect("failed to create dir");
    repo.write("nested/inner.txt", "1\n");
    repo.commit("c1");

    let out = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "nested", "--json"])
        .output()
        .expect("failed to run repoprism");

    // `cat-file -s` 对目录同样成功（返回 29），所以这条走的是**第二次**调用失败的路径。
    assert!(!out.status.success(), "目录不能被当成文件预览");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("不是普通文件"),
        "必须说出失败原因，stderr={stderr}"
    );
}

#[test]
fn blob_rejects_an_empty_file_path() {
    let repo = TempRepo::new("blob-empty");
    repo.write("a.txt", "1\n");
    repo.commit("c1");

    let out = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file=", "--json"])
        .output()
        .expect("failed to run repoprism");

    assert!(!out.status.success(), "空路径必须非零退出");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("不能为空"),
        "空路径要被自己的校验挡住，而不是交给 git"
    );
}

#[test]
fn blob_human_output_names_the_kind_without_dumping_base64() {
    let repo = TempRepo::new("blob-image");
    write_bytes(&repo, "pic.png", PNG);
    repo.commit("c1");

    let as_json = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "pic.png", "--json"])
        .output()
        .expect("failed to run repoprism");
    assert!(as_json.status.success());

    let data = json_of(&as_json.stdout)["data"].clone();
    assert_eq!(data["kind"]["kind"], "image");
    assert_eq!(data["kind"]["format"], "png", "判据是魔法字节不是扩展名");
    let b64 = data["content"]
        .as_str()
        .expect("图片应给 base64")
        .to_string();

    let human = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "pic.png"])
        .output()
        .expect("failed to run repoprism");
    let stdout = String::from_utf8_lossy(&human.stdout);

    assert!(stdout.contains("kind: image"), "stdout={stdout}");
    assert!(stdout.contains("image/png"), "应给出 MIME，stdout={stdout}");
    assert!(
        !stdout.contains(&b64),
        "base64 是给程序用的，不该把终端塞满：stdout={stdout}"
    );
}

#[test]
fn blob_human_output_says_the_lfs_content_is_not_here() {
    let repo = TempRepo::new("blob-lfs");
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a2146\nsize 12345\n";
    repo.write("big.bin", pointer);
    repo.commit("c1");

    let human = repoprism()
        .arg("blob")
        .arg(repo.path())
        .args(["--file", "big.bin"])
        .output()
        .expect("failed to run repoprism");
    let stdout = String::from_utf8_lossy(&human.stdout);

    assert!(stdout.contains("lfs_pointer"), "stdout={stdout}");
    assert!(stdout.contains("4d7a2146"), "应给出 oid，stdout={stdout}");
    assert!(
        stdout.contains("12345"),
        "应给出 LFS 对象的真实大小，stdout={stdout}"
    );
    assert!(
        stdout.contains("未下载"),
        "必须说明内容不在仓库里，stdout={stdout}"
    );
}
