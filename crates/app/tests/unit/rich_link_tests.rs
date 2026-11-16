use super::*;

/// 富文本点击目标分类：外链进浏览器标签，本地路径进文件预览，危险 scheme 一律拒绝。
///
/// 数据构造：覆盖 https、mailto、`norma-path:` 内部链接（含百分号编码的空格与中文）、
/// file://、无 scheme 的相对链接、javascript:、data: 各一例。
///
/// 执行过程：对每个输入调用纯函数 `classify_target`。
///
/// 预期结果：见各断言；javascript:/data: 必须是 Reject，避免消息文本触发脚本或内嵌页面。
#[test]
fn test_classify_target_routes_web_path_and_rejects_dangerous_schemes() {
    assert_eq!(
        classify_target("https://example.com/a?b=1"),
        RichTarget::Web("https://example.com/a?b=1".into()),
        "https 链接应作为外链"
    );
    assert_eq!(
        classify_target("mailto:a@b.co"),
        RichTarget::System("mailto:a@b.co".into()),
        "mailto 交给系统，不进抽屉浏览器"
    );
    assert_eq!(
        classify_target("norma-path:/Users/x/my%20dir/%E8%9B%87.html"),
        RichTarget::Path("/Users/x/my dir/蛇.html".into()),
        "内部链接应还原空格与中文"
    );
    assert_eq!(
        classify_target("file:///tmp/a.png"),
        RichTarget::Path("/tmp/a.png".into()),
        "file:// 视为本地路径"
    );
    assert_eq!(
        classify_target("docs/a.md"),
        RichTarget::Path("docs/a.md".into()),
        "无 scheme 的相对链接按路径处理"
    );
    assert_eq!(
        classify_target("javascript:alert(1)"),
        RichTarget::Reject,
        "javascript: 必须拒绝"
    );
    assert_eq!(
        classify_target("data:text/html,<b>x</b>"),
        RichTarget::Reject,
        "data: 必须拒绝"
    );
}

/// 行内代码路径改写：只改写 Markdown 行内代码里的路径，保留行列后缀与其它文本。
///
/// 数据构造：一段含绝对路径、带 `:12` 行号的相对路径、非路径代码 `cargo test`、
/// 围栏代码块里的路径、已有链接里的代码路径。
///
/// 执行过程：调用 `linkify_inline_paths`。
///
/// 预期结果：仅前两处被改写为 `[`..`](norma-path:..)`；其余字节不变。
#[test]
fn test_linkify_inline_paths_only_rewrites_inline_code_paths() {
    let source = "见 `/Users/x/snake.html` 与 `src/lib.rs:12`，运行 `cargo test`。\n\
                  ```\n`/not/linked.html`\n```\n\
                  [`/in/link.md`](https://e.com)";
    let out = linkify_inline_paths(source);

    assert!(
        out.contains("[`/Users/x/snake.html`](norma-path:/Users/x/snake.html)"),
        "绝对路径应被改写: {out}"
    );
    assert!(
        out.contains("[`src/lib.rs:12`](norma-path:src/lib.rs:12)"),
        "行号后缀保留在目标里，由路径解析层剥离: {out}"
    );
    assert!(out.contains("`cargo test`"), "非路径代码不改写: {out}");
    assert!(
        !out.contains("](norma-path:/not/linked.html)"),
        "围栏代码块内不改写: {out}"
    );
    assert!(
        out.contains("[`/in/link.md`](https://e.com)") && !out.contains("norma-path:/in/link.md"),
        "已在链接里的代码不重复改写: {out}"
    );
}

/// 路径含空格或括号时目标须百分号编码，保证 Markdown 目标合法且可无损还原。
///
/// 数据构造：`/a b/(c).md`。执行过程：改写后再 classify。预期：往返还原为原路径。
#[test]
fn test_linkify_inline_paths_round_trips_special_characters() {
    let out = linkify_inline_paths("`/a b/(c).md`");
    let start = out.find("](").expect("应生成链接") + 2;
    let target = &out[start..out.len() - 1];

    assert!(
        !target.contains(' ') && !target.contains('(') && !target.contains(')'),
        "目标不得含空格或括号: {target}"
    );
    assert_eq!(
        classify_target(target),
        RichTarget::Path("/a b/(c).md".into()),
        "往返后应还原原始路径"
    );
}

/// 裸 URL 与 Markdown 图片改写：裸 URL 变自动链接，图片套同目标链接，已有链接/代码保持原样。
///
/// 数据构造：句末带标点的裸 URL、已是 `[t](url)` 的链接、行内代码里的 URL、本地图片与远程图片。
///
/// 执行过程：调用 `linkify_inline_paths`。
///
/// 预期结果：只有裸 URL 被 `<>` 包裹且不含句末标点；图片被包成可点击链接。
#[test]
fn test_linkify_wraps_bare_urls_and_images() {
    assert_eq!(
        linkify_inline_paths(
            "看 https://a.com/x?y=1&z=2。再看 [t](https://b.com) 与 `https://c.com`"
        ),
        "看 <https://a.com/x?y=1&z=2>。再看 [t](https://b.com) 与 `https://c.com`",
        "仅裸 URL 被包裹，句号不入链接"
    );
    assert_eq!(
        linkify_inline_paths("![图](/tmp/a.png) ![r](https://x.io/i.png)"),
        "[![图](/tmp/a.png)](norma-path:/tmp/a.png) [![r](https://x.io/i.png)](https://x.io/i.png)",
        "图片应套同目标链接"
    );
}

/// 用户消息纯文本：Markdown 语法按字面转义，仅裸 URL 变可点链接。
///
/// 数据构造：含 `*粗体*`、`# 标题` 与句末带标点的 URL。
///
/// 执行过程：调用 `linkify_plain_text`。
///
/// 预期结果：标点被反斜杠转义，URL 被 `<>` 包裹且不含句末标点。
#[test]
fn test_linkify_plain_text_escapes_markdown_but_links_urls() {
    assert_eq!(
        linkify_plain_text("*a* # b https://x.io/a_b."),
        "\\*a\\* \\# b <https://x.io/a_b>\\.",
        "语法转义，URL 原样"
    );
}

/// 对抗审查回归：scheme 判定含 `-`/`.`/数字、控制字符、`file://localhost`、行号后缀，
/// 以及系统打开的可执行类型黑名单与 URL 词边界。
///
/// 数据构造：`vscode-insiders://x`、`java\nscript:x`、`http:foo`、`file://localhost/tmp/a`、`lib.rs:12`、`foohttps://x.io`。
///
/// 执行过程：调用 `classify_target`、`is_unsafe_for_system_open`、`linkify_inline_paths`。
///
/// 预期结果：未知 scheme/控制字符/残缺 http 拒绝；行号后缀仍是路径；`.app`/`.command` 不放行；词内 http 不成链接。
#[test]
fn test_adversarial_review_regressions() {
    for bad in [
        "vscode-insiders://x",
        "java\nscript:x",
        "http:foo",
        "x-apple.systempreferences:a",
    ] {
        assert_eq!(classify_target(bad), RichTarget::Reject, "{bad:?} 必须拒绝");
    }
    assert_eq!(
        classify_target("file://localhost/tmp/a"),
        RichTarget::Path("/tmp/a".into())
    );
    assert_eq!(
        classify_target("lib.rs:12"),
        RichTarget::Path("lib.rs:12".into())
    );
    for p in ["/Applications/Foo.app", "/tmp/x.COMMAND", "/a/b.dmg"] {
        assert!(
            is_unsafe_for_system_open(std::path::Path::new(p)),
            "{p} 不应交给系统"
        );
    }
    assert!(!is_unsafe_for_system_open(std::path::Path::new("/a/b.pdf")));
    assert_eq!(linkify_inline_paths("foohttps://x.io"), "foohttps://x.io");
    assert_eq!(linkify_plain_text("a\nb"), "a\\\nb", "换行保留为硬换行");
}
