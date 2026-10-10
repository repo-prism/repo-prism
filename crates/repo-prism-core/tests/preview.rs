//! blob 只读预览的集成测试（US-3，补丁 P-10）。
//!
//! # 为什么必须真的提交二进制文件
//!
//! `cat-file` 读到的字节是什么形状，取决于 **git 把它怎么存进对象库**，
//! 而不是我们怎么构造那个 `Vec<u8>`。只用纯函数判类型，证明的是
//! 「我们对 magic bytes 的记忆与代码一致」，证明不了
//! 「一个真实文件经过 git 往返之后还是那些字节」。
//! 所以本文件的每个夹具都真的 `git add` 一个二进制文件，
//! 再从 `Git::blob_preview` 读回来断言。
//!
//! # 这个文件钉住的四条性质
//!
//! 1. **类型判定与文件名无关** —— 内容是 PNG、名字叫 `.txt` 的必须是图片；反之亦然。
//! 2. **先看大小再决定读不读** —— 超过上限时 `blob_preview()` 只起 **1** 次子进程
//!    （第二次压根没发生）。这条断言本身就是证明：起 2 次就说明内容被读回来了。
//! 3. **「几乎没有预览价值」的输入也要说实话** —— 目录与缺失路径都不能降级成
//!    「一个大小为 0 的空文件」，而必须报错并说明原因。
//! 4. **旧版本与新版本都要能取到** —— 前端的「图片对比」依赖这条。

mod common;

use base64::Engine as _;
use common::TempRepo;
use repo_prism_core::{BlobKind, Git, ImageFormat};

/// 把一小段原始字节包成一个 zlib 流（**未压缩的 stored block**）。
///
/// 用它而不是随便编几个字节，是为了让下面的 PNG 夹具是一张**真能渲染**的图：
/// 「图片预览」这条验收标准的落点在浏览器那边，给它一个渲染不出来的输入，
/// 等于什么都没验。
fn stored_zlib(raw: &[u8]) -> Vec<u8> {
    // CMF=0x78 / FLG=0x01 是 32K 窗口下的标准组合（0x7801 % 31 == 0）
    let mut out = vec![0x78, 0x01];
    // bit0 = BFINAL=1、bit1..2 = BTYPE=00（stored），其余位补 0
    out.push(0x01);
    let len = raw.len() as u16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(raw);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in raw {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

/// CRC-32（PNG 每个块尾都要它）。二十来行，不值得为它加一个依赖。
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// 一个 1×1 的真彩色 PNG。颜色由 `fill` 决定（哪怕是同一个构图，字节也不同）。
fn png_bytes(fill: u8) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes()); // 宽
    ihdr.extend_from_slice(&1u32.to_be_bytes()); // 高
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 位深 8 / 彩色类型 2

    let idat = stored_zlib(&[0x00, fill, fill, fill]);

    let mut chunks = Vec::new();
    for (tag, data) in [
        (b"IHDR".as_slice(), ihdr.as_slice()),
        (b"IDAT", idat.as_slice()),
        (b"IEND", [].as_slice()),
    ] {
        let mut body = tag.to_vec();
        body.extend_from_slice(data);
        let mut bytes = (data.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(&body);
        bytes.extend_from_slice(&crc32(&body).to_be_bytes());
        chunks.push(bytes);
    }
    for chunk in chunks {
        out.extend_from_slice(&chunk);
    }
    out
}

fn sample(name: &str) -> Vec<u8> {
    match name {
        "png" => png_bytes(0xff),
        "jpeg" => vec![
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01,
        ],
        "gif" => b"GIF89a".to_vec(),
        // 必须凑够 12 字节：判据要读第 8–12 字节
        "webp" => b"RIFF\x1e\x00\x00\x00WEBP".to_vec(),
        "bmp" => b"BM\x28\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        other => panic!("未知样本 {other}"),
    }
}

fn base64_decode(text: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .expect("base64 必须是标准字母表 —— 否则前端的 data URL 会是一团噪声")
}

/// 顺手确认一下 PNG 夹具真的是一张合法图片，而不是「头字节刚好对」。
///
/// 这条断言只能住在这里（core 里没有 PNG 解码器），但它的价值很大：
/// 若 `stored_zlib` / `crc32` 写错，真实浏览器会显示「损坏的图片」，
/// 而往返字节一致的断言照样会是绿的。
#[test]
fn the_png_fixture_is_a_real_png() {
    let bytes = png_bytes(0xff);
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(
        u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize,
        13
    );
    assert_eq!(&bytes[12..16], b"IHDR");
    // IDAT 必须能解压回原始扫描行（滤波字节 + RGB）
    let idat_len = u32::from_be_bytes(bytes[33..37].try_into().unwrap()) as usize;
    let idat = &bytes[41..41 + idat_len];
    let inflated = inflate(idat);
    assert_eq!(inflated, vec![0x00, 0xff, 0xff, 0xff]);
    assert!(bytes.windows(4).any(|w| w == b"IEND"));
}

/// 上面那次校验要用到的极简 inflate：只认 stored block（也就是我们自己写的那种）。
fn inflate(stream: &[u8]) -> Vec<u8> {
    assert_eq!(&stream[..2], &[0x78, 0x01], "zlib 头");
    assert_eq!(stream[2], 0x01, "BFINAL + stored");
    let len = u16::from_le_bytes(stream[3..5].try_into().unwrap()) as usize;
    assert_eq!(
        u16::from_le_bytes(stream[5..7].try_into().unwrap()),
        !(len as u16),
        "LEN / NLEN 必须互补"
    );
    stream[7..7 + len].to_vec()
}

#[test]
fn every_supported_image_is_read_back_byte_for_byte() {
    let repo = TempRepo::new("preview-images");
    let expected = [
        ("png", ImageFormat::Png),
        ("jpeg", ImageFormat::Jpeg),
        ("gif", ImageFormat::Gif),
        ("webp", ImageFormat::Webp),
        ("bmp", ImageFormat::Bmp),
    ];
    for (name, _) in &expected {
        repo.write_bytes(&format!("assets/{name}.bin"), &sample(name));
    }
    repo.commit("add images");

    let git = Git::open(repo.path()).expect("open repo");
    let head = repo.head_sha();

    for (name, format) in expected {
        let preview = git
            .blob_preview(&head, &format!("assets/{name}.bin"))
            .unwrap_or_else(|e| panic!("预览 {name} 失败：{e}"));
        assert_eq!(
            preview.kind,
            BlobKind::Image { format },
            "{name} 应被识别为 {format:?}"
        );
        // 往返一致：这是「进了 git 又出来，字节没被换过」的唯一证明
        let round_tripped = base64_decode(preview.content.as_deref().expect("图片必须有内容"));
        assert_eq!(round_tripped, sample(name), "{name} 的字节必须逐字节一致");
        assert_eq!(
            preview.size,
            sample(name).len() as u64,
            "{name} 大小必须真实"
        );
    }
}

#[test]
fn svg_is_rendered_as_an_image_with_an_svg_media_type() {
    let repo = TempRepo::new("preview-svg");
    let svg = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"1\" height=\"1\"/></svg>";
    repo.write_bytes("logo.svg", svg);
    repo.commit("add svg");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git
        .blob_preview(&repo.head_sha(), "logo.svg")
        .expect("preview");
    assert_eq!(
        preview.kind,
        BlobKind::Image {
            format: ImageFormat::Svg
        }
    );
    assert_eq!(
        base64_decode(preview.content.as_deref().expect("SVG 也是图片")),
        svg.to_vec()
    );
}

#[test]
fn the_kind_comes_from_the_bytes_not_the_name() {
    let repo = TempRepo::new("preview-mismatch");
    // 内容是 PNG，名字却是 .txt
    repo.write_bytes("actually-a-picture.txt", &png_bytes(0x11));
    // 内容是文本，名字却是 .png
    repo.write("actually-text.png", "这真的只是一句话\n");
    repo.commit("confusing names");

    let git = Git::open(repo.path()).expect("open repo");
    let head = repo.head_sha();

    let as_image = git
        .blob_preview(&head, "actually-a-picture.txt")
        .expect("preview");
    assert!(
        matches!(as_image.kind, BlobKind::Image { .. }),
        "内容是 PNG 就必须判为图片，不管它叫什么。实际：{:?}",
        as_image.kind
    );

    let as_text = git
        .blob_preview(&head, "actually-text.png")
        .expect("preview");
    assert_eq!(
        as_text.kind,
        BlobKind::Text,
        "内容是文本就必须判为文本，不管它叫什么"
    );
    assert_eq!(as_text.text.as_deref(), Some("这真的只是一句话\n"));
}

#[test]
fn an_lfs_pointer_shows_the_oid_without_downloading_the_content() {
    let repo = TempRepo::new("preview-lfs");
    let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
    repo.write_bytes("huge.bin", pointer);
    repo.commit("add a pointer");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git
        .blob_preview(&repo.head_sha(), "huge.bin")
        .expect("preview");

    assert_eq!(
        preview.kind,
        BlobKind::LfsPointer {
            oid: "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393".to_string(),
            size: 12_345,
        },
        "指针里的 size 是**真实文件**的大小，要展示给用户"
    );
    assert_eq!(
        preview.size,
        pointer.len() as u64,
        "而 preview.size 是**指针文件本身**的字节数 —— 两者不是一回事，都要对"
    );
    assert!(
        preview.content.is_none(),
        "指针不能伪装成普通内容：它指向的东西从来没被下载过"
    );
}

#[test]
fn an_oversized_blob_is_measured_but_never_read() {
    let repo = TempRepo::new("preview-too-large");
    let mut huge = png_bytes(0x22);
    huge.extend(std::iter::repeat_n(0x00, 5 * 1024 * 1024));
    repo.write_bytes("big.png", &huge);
    repo.commit("add something huge");

    let git = Git::open(repo.path()).expect("open repo");
    let head = repo.head_sha();

    let before = git.spawns();
    let size = git
        .blob_size(&head, "big.png")
        .expect("blob_size")
        .expect("big.png 存在");
    assert_eq!(git.spawns() - before, 1, "blob_size() 只做一次大小查询");
    assert_eq!(size, huge.len() as u64, "大小必须是真实值");

    // 本卡最关键的一条断言：预览也只起**一次**。
    // 起两次就意味着内容真的被读回来了，「先看大小再决定读不读」就成了空话。
    let before = git.spawns();
    let preview = git.blob_preview(&head, "big.png").expect("preview");
    assert_eq!(
        git.spawns() - before,
        1,
        "超过上限时 blob_preview() 不得读内容 —— 起两次就说明它读了"
    );
    assert!(preview.too_large, "必须显式标记「太大没读」");
    assert_eq!(preview.kind, BlobKind::TooLarge);
    assert_eq!(preview.size, huge.len() as u64, "就算不读也要给出真实大小");
    assert!(preview.content.is_none() && preview.text.is_none() && preview.hex.is_none());
    assert!(
        !preview.truncated,
        "truncated 与 too_large 是两件事，这里一个字节都没读到，谈不上「呈现被截断」"
    );
}

#[test]
fn unknown_binary_is_shown_as_a_hex_dump() {
    let repo = TempRepo::new("preview-hex");
    // 9 字节：既不是合法 UTF-8（0xff），又短到一行能装下，便于写死断言
    let mut bytes = vec![0x00, 0x01, 0x02, 0xfe, 0xff];
    bytes.extend(b"tail");
    repo.write_bytes("blob.bin", &bytes);
    repo.commit("add unknown binary");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git
        .blob_preview(&repo.head_sha(), "blob.bin")
        .expect("preview");
    assert_eq!(preview.kind, BlobKind::Binary);
    let dump = preview.hex.as_deref().expect("未知二进制要有十六进制转储");
    assert_eq!(dump, "00000000  00 01 02 fe ff 74 61 69 6c  |.....tail|\n");
    assert_eq!(preview.size, bytes.len() as u64);
}

#[test]
fn a_del_byte_is_still_text_because_it_is_valid_utf8() {
    // 0x7f（DEL）虽然不可打印，却是**合法的单字节 UTF-8**。
    // 这条边界值得钉住：若哪天为了「好看」把控制字符一律按二进制处理，
    // 一段含退格的普通文本就会被送去十六进制转储。
    let repo = TempRepo::new("preview-del");
    repo.write_bytes("del.txt", &[b'a', 0x7f, b'b']);
    repo.commit("add control chars");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git
        .blob_preview(&repo.head_sha(), "del.txt")
        .expect("preview");
    assert_eq!(preview.kind, BlobKind::Text);
    assert!(preview.hex.is_none());
}

#[test]
fn a_long_hex_dump_says_it_was_cut() {
    let repo = TempRepo::new("preview-hex-cut");
    // 用 0x80 而不是 0x7f：后者是合法 UTF-8，会被判成文本（见上一条）
    repo.write_bytes("blob.bin", &vec![0x80; 4096]);
    repo.commit("add bigger binary");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git
        .blob_preview(&repo.head_sha(), "blob.bin")
        .expect("preview");
    assert!(preview.truncated, "用户必须知道后面还有");
    assert_eq!(preview.size, 4096, "给出的是**真实**大小，不是转储的大小");
    assert_eq!(
        preview.hex.as_deref().expect("转储").lines().count(),
        512 / 16,
        "十六进制转储上限 512 字节 = 32 行"
    );
}

#[test]
fn a_long_text_file_says_it_was_cut() {
    let repo = TempRepo::new("preview-text-cut");
    // 一个「中」三字节，30 万个就超过 256 KiB 的文本呈现上限
    repo.write("big.txt", &"中".repeat(300_000));
    repo.commit("add a long text");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git
        .blob_preview(&repo.head_sha(), "big.txt")
        .expect("preview");
    assert_eq!(preview.kind, BlobKind::Text);
    assert!(preview.truncated, "超长文本必须显式标记截断");
    let text = preview.text.as_deref().expect("文本要有内容");
    assert!(text.len() <= 256 * 1024);
    assert!(
        text.chars().all(|c| c == '中'),
        "截断必须落在字符边界上，不能切开多字节字符"
    );
    assert_eq!(preview.size, 900_000, "size 永远是仓库里的真实字节数");
}

#[test]
fn both_sides_of_a_modified_image_can_be_read() {
    let repo = TempRepo::new("preview-compare");
    repo.write_bytes("logo.png", &png_bytes(0x10));
    repo.commit("first version");
    let old = repo.head_sha();

    repo.write_bytes("logo.png", &png_bytes(0xee));
    repo.commit("second version");
    let new = repo.head_sha();

    let git = Git::open(repo.path()).expect("open repo");
    let old_view = git.blob_preview(&old, "logo.png").expect("old version");
    let new_view = git.blob_preview(&new, "logo.png").expect("new version");

    let old_bytes = base64_decode(old_view.content.as_deref().expect("图片有内容"));
    let new_bytes = base64_decode(new_view.content.as_deref().expect("图片有内容"));
    assert_eq!(old_bytes, png_bytes(0x10));
    assert_eq!(new_bytes, png_bytes(0xee));
    assert_ne!(
        old_bytes, new_bytes,
        "两个版本必须真的不同 —— 否则前端的「对比」看到的是同一张图"
    );
}

#[test]
fn a_directory_refuses_to_be_shown_as_a_file() {
    let repo = TempRepo::new("preview-dir");
    repo.write("nested/inner.txt", "hello\n");
    repo.commit("add nested");

    let git = Git::open(repo.path()).expect("open repo");
    let head = repo.head_sha();

    // `cat-file -s` 对目录同样成功（实测返回 29），危险正在这里：
    // 若只信这个数字，用户会看到一个「29 字节」的文件。
    assert!(
        git.blob_size(&head, "nested").expect("blob_size").is_some(),
        "确认这一事实：`-s` 对目录确实返回一个数"
    );

    let error = git
        .blob_preview(&head, "nested")
        .expect_err("目录不能被当成文件预览");
    let message = error.to_string();
    assert!(
        message.contains("不是普通文件"),
        "错误信息要说清原因，实际：{message}"
    );
}

#[test]
fn a_missing_path_and_a_dangerous_version_are_rejected_outright() {
    let repo = TempRepo::new("preview-invalid");
    repo.write("a.txt", "hello\n");
    repo.commit("init");

    let git = Git::open(repo.path()).expect("open repo");
    let head = repo.head_sha();

    assert!(
        git.blob_preview(&head, "nope.txt").is_err(),
        "不存在的路径必须报错，而不是给一个空文件"
    );
    assert!(
        git.blob_preview("deadbeef", "a.txt").is_err(),
        "不存在的版本必须报错"
    );
    assert!(
        git.blob_preview("-assume-unchanged", "a.txt").is_err(),
        "以连字符开头的版本会被 git 当成选项，必须拒绝"
    );
    assert!(
        git.blob_preview(&head, "-a.txt").is_err(),
        "以连字符开头的路径同理"
    );
    assert!(git.blob_preview(&head, "").is_err(), "空路径必须拒绝");
}

// --- 线格式（P-11）---------------------------------------------------------
// P-10 只测了 Rust 结构，**没有任何测试断言过它的 JSON 形状**，
// 而桌面端 / CLI / MCP 三处消费者拿到的都是 JSON。
// 前端的 `src/lib/api.ts` 里那份 `BlobKind` 是手写的，与 serde 之间没有编译期联系：
// 改一个 `rename_all` 或一个字段名，Rust 测试全绿、UI 静默变空白。
// 下面几条把线上形状钉住，让这类漂移至少在 Rust 侧会红。

#[test]
fn every_kind_carries_a_snake_case_tag_on_the_wire() {
    let cases = [
        (
            BlobKind::Image {
                format: ImageFormat::Png,
            },
            "image",
        ),
        (BlobKind::Text, "text"),
        (BlobKind::Binary, "binary"),
        (
            BlobKind::LfsPointer {
                oid: "abc".to_string(),
                size: 1,
            },
            "lfs_pointer",
        ),
        (BlobKind::TooLarge, "too_large"),
    ];

    for (kind, tag) in cases {
        let value = serde_json::to_value(&kind).expect("serialize");
        assert_eq!(
            value["kind"], tag,
            "tag 的形状由 rename_all 决定，前端按它 switch，实际 {value}"
        );
    }
}

#[test]
fn a_preview_serializes_to_a_flat_envelope_with_a_nested_kind() {
    let repo = TempRepo::new("preview-wire-text");
    repo.write("a.txt", "hello\n");
    repo.commit("c1");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git.blob_preview("HEAD", "a.txt").expect("preview");
    let value = serde_json::to_value(&preview).expect("serialize");

    // `Value::Object` 是 BTreeMap，键是字母序而不是声明序（与 `to_string` 不同），
    // 所以这里断言的是**集合**而不是顺序 —— 顺序本来也不该被依赖。
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "content",
            "hex",
            "kind",
            "size",
            "text",
            "too_large",
            "truncated"
        ],
        "字段集合变了就是 breaking change（前端按名字取值），实际 {keys:?}"
    );
    assert!(
        value["kind"].is_object(),
        "kind 是带 tag 的枚举，线上是**嵌套对象**（`kind.kind`），实际 {value}"
    );
}

#[test]
fn an_image_preview_carries_base64_under_content() {
    let repo = TempRepo::new("preview-wire-image");
    repo.write_bytes("pic.png", &png_bytes(0x33));
    repo.commit("c1");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git.blob_preview("HEAD", "pic.png").expect("preview");
    let value = serde_json::to_value(&preview).expect("serialize");

    assert_eq!(value["kind"]["kind"], "image");
    assert_eq!(value["kind"]["format"], "png");
    let b64 = value["content"].as_str().expect("content 应为字符串");
    assert!(
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .is_ok(),
        "content 必须是可解码的 base64，否则前端拼出的 data URL 渲染不出来"
    );
    assert!(
        value["text"].is_null() && value["hex"].is_null(),
        "图片不给文本与转储"
    );
}

#[test]
fn a_preview_round_trips_through_json_without_losing_a_field() {
    let repo = TempRepo::new("preview-wire-roundtrip");
    let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
    repo.write_bytes("huge.bin", pointer);
    repo.commit("c1");

    let git = Git::open(repo.path()).expect("open repo");
    let preview = git.blob_preview("HEAD", "huge.bin").expect("preview");

    let text = serde_json::to_string(&preview).expect("serialize");
    let back: repo_prism_core::BlobPreview = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(
        back, preview,
        "序列化必须无损：丢一个字段，另一端的『没读到』就变成『读到了空』"
    );

    // 顺带钉住那两个同名的 size —— 它们**不是**一回事，名字相同纯属历史形状。
    let value = serde_json::to_value(&preview).expect("serialize");
    assert_eq!(
        value["size"],
        pointer.len() as u64,
        "外层是**指针文件**的大小"
    );
    assert_eq!(value["kind"]["size"], 12_345, "内层是 LFS **对象**的大小");
    assert_ne!(
        value["size"], value["kind"]["size"],
        "两者相等反而说明形状错了"
    );
}
