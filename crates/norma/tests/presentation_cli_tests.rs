use norma::cli::{
    parse_invocation_from, render_protocol_error, render_protocol_success, OutputFormat,
};
use norma::cli_runtime::run_invocation;
use norma::presentation::{
    prepare_presentation_target, validate_prepared_target, PresentationPreparationError,
};
use norma::protocol::{
    CardCommand, CardCommandError, CardCommandResult, PresentationDisposition,
    PresentationErrorCode, PresentationKind, PresentationLayout, PresentationTarget, PreviewType,
};
use std::fs;

/// Present CLI 命令树：Browser/Preview 应映射为独立高层命令并默认输出 JSON。
///
/// 数据构造（含关键数值的推导过程）：
///   命令数 = 2：browser + preview
///   Browser URL = "docs.example.com/guide"（无 scheme，留给执行前规范化）
///   Preview path = "./README.md"；显式类型 = Markdown
///   `--new` 次数 = 1，仅 Browser 强制创建副本
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 `norma present browser ... --new`。
///   2. 解析 `norma present preview ... --type markdown`。
///   3. 检查两条命令的目标、create_new 与默认输出格式。
///
/// 预期结果：
///   - 正断言：Browser 和 Preview 都生成单目标 Present 命令。
///   - 正断言：Present 默认使用 JSON，Browser 保留显式 `--new`。
///   - 负断言：Preview 未明确要求副本时不得隐式 create_new。
#[test]
fn present_command_tree_parses_browser_and_preview_with_json_default() {
    let browser = parse_invocation_from([
        "norma",
        "present",
        "browser",
        "docs.example.com/guide",
        "--new",
    ])
    .expect("present browser 应解析为公开 CLI 调用");
    let preview = parse_invocation_from([
        "norma",
        "present",
        "preview",
        "./README.md",
        "--type",
        "markdown",
    ])
    .expect("present preview 应解析路径与显式类型");

    assert_eq!(
        browser.command,
        CardCommand::Present {
            target: PresentationTarget::Browser {
                url: "docs.example.com/guide".to_string(),
            },
            create_new: true,
        },
        "present browser 必须保留原始 URL，并把 --new 映射为 create_new=true"
    );
    assert_eq!(
        browser.format,
        OutputFormat::Json,
        "Present 命令未指定 format 时必须默认输出稳定 JSON"
    );
    assert_eq!(
        preview.command,
        CardCommand::Present {
            target: PresentationTarget::Preview {
                path: "./README.md".into(),
                explicit_type: Some(PreviewType::Markdown),
            },
            create_new: false,
        },
        "present preview 必须保留路径、类型并默认允许复用"
    );
    assert!(
        !matches!(
            preview.command,
            CardCommand::Present {
                create_new: true,
                ..
            }
        ),
        "未指定 --new 的 Preview 不得强制创建副本"
    );
}

/// Browser 规范化边界：默认首页、裸地址、自定义 scheme 可用，危险 scheme 必须拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   合法输入数 = 7：空 URL、裸 host、localhost:port、domain:port、IPv6:port、file、自定义 x-apple
///   非法输入数 = 2：data + javascript
///   空 URL 默认值 = https://www.google.com
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别准备九个 Browser presentation target。
///   2. 对无 scheme 输入补 `https://`，包括常见的 host:port 与 IPv6:port。
///   3. 保留已有安全 scheme 与自定义 scheme。
///   4. 在连接 App 前拒绝两个可执行内容 scheme。
///
/// 预期结果：
///   - 正断言：七个安全输入得到确定的规范化 URL。
///   - 正断言：host:port 不得被误判成自定义 scheme。
///   - 正断言：data/javascript 返回 ForbiddenUrlScheme。
///   - 负断言：危险输入不得被补成看似安全的 https URL。
#[test]
fn browser_preparation_normalizes_safe_urls_and_rejects_executable_schemes() {
    let default = prepare_presentation_target(PresentationTarget::Browser { url: String::new() })
        .expect("空 Browser URL 应使用产品默认首页");
    let bare = prepare_presentation_target(PresentationTarget::Browser {
        url: " docs.example.com/guide ".to_string(),
    })
    .expect("裸 host 应补全 https scheme");
    let localhost_port = prepare_presentation_target(PresentationTarget::Browser {
        url: "localhost:3000".to_string(),
    })
    .expect("localhost:port 应被识别为无 scheme 的开发地址");
    let domain_port = prepare_presentation_target(PresentationTarget::Browser {
        url: "example.com:8080/review".to_string(),
    })
    .expect("domain:port 应被识别为无 scheme 的网络地址");
    let ipv6_port = prepare_presentation_target(PresentationTarget::Browser {
        url: "[::1]:4173".to_string(),
    })
    .expect("IPv6:port 应被识别为无 scheme 的开发地址");
    let file = prepare_presentation_target(PresentationTarget::Browser {
        url: "file:///tmp/report.html".to_string(),
    })
    .expect("file scheme 应保留给 Browser 展示");
    let custom = prepare_presentation_target(PresentationTarget::Browser {
        url: "x-apple.systempreferences:com.apple.preference.security".to_string(),
    })
    .expect("系统支持的自定义 scheme 应通过校验");
    let data = prepare_presentation_target(PresentationTarget::Browser {
        url: "data:text/html,unsafe".to_string(),
    });
    let javascript = prepare_presentation_target(PresentationTarget::Browser {
        url: "javascript:alert(1)".to_string(),
    });
    let default_is_server_valid = validate_prepared_target(&default);

    assert_eq!(
        default,
        PresentationTarget::Browser {
            url: "https://www.google.com".to_string(),
        },
        "空 URL 必须稳定映射到 Google 默认首页"
    );
    assert_eq!(
        bare,
        PresentationTarget::Browser {
            url: "https://docs.example.com/guide".to_string(),
        },
        "无 scheme 地址必须只补一个 https:// 前缀"
    );
    assert_eq!(
        localhost_port,
        PresentationTarget::Browser {
            url: "https://localhost:3000/".to_string(),
        },
        "localhost:port 必须补 https://，不能把 localhost 当成自定义 scheme"
    );
    assert_eq!(
        domain_port,
        PresentationTarget::Browser {
            url: "https://example.com:8080/review".to_string(),
        },
        "domain:port 必须补 https://，不能把域名当成自定义 scheme"
    );
    assert_eq!(
        ipv6_port,
        PresentationTarget::Browser {
            url: "https://[::1]:4173/".to_string(),
        },
        "IPv6:port 必须补 https:// 并保留方括号与端口"
    );
    assert_eq!(
        file,
        PresentationTarget::Browser {
            url: "file:///tmp/report.html".to_string(),
        },
        "file URL 不得被改写为 https"
    );
    assert_eq!(
        custom,
        PresentationTarget::Browser {
            url: "x-apple.systempreferences:com.apple.preference.security".to_string(),
        },
        "自定义系统 scheme 应保持原值"
    );
    assert_eq!(
        data,
        Err(PresentationPreparationError::ForbiddenUrlScheme {
            scheme: "data".to_string(),
        }),
        "data scheme 必须在任何 App 副作用前被拒绝"
    );
    assert_eq!(
        javascript,
        Err(PresentationPreparationError::ForbiddenUrlScheme {
            scheme: "javascript".to_string(),
        }),
        "javascript scheme 必须在任何 App 副作用前被拒绝"
    );
    assert_eq!(
        default_is_server_valid,
        Ok(()),
        "CLI 生成的无尾斜杠默认首页必须能通过 App 的同一份校验"
    );
    assert!(
        !matches!(
            data,
            Ok(PresentationTarget::Browser { ref url })
                if url.starts_with("https://")
        ),
        "危险 scheme 不得被错误补全为 https URL"
    );
}

/// Present 本地预检错误：缺失 Preview 必须在连接 App 前输出稳定 JSON。
///
/// 数据构造（含关键数值的推导过程）：
///   missing path = 临时目录下 missing.md（明确不创建）
///   App socket = 不启动
///   期望错误码 = preview_not_found；确定性请求错误退出码=5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 解析 present preview 缺失路径，默认选择 JSON。
///   2. run_invocation 在客户端 canonicalize 阶段发现 NotFound。
///   3. 在尝试 Provider 凭据或 Socket 连接前渲染协议错误。
///
/// 预期结果：
///   - 正断言：stdout 是 ok=false、code=preview_not_found，exit_code=5。
///   - 正断言：stderr 为空，证明协议输出没有混入诊断。
///   - 负断言：错误不得退化为 app_unavailable 或 internal_error。
#[test]
fn missing_preview_returns_stable_json_before_app_connection() {
    let temp = tempfile::tempdir().expect("测试应创建隔离缺失路径根目录");
    let missing = temp.path().join("missing.md");
    let invocation = parse_invocation_from([
        "norma",
        "present",
        "preview",
        missing.to_string_lossy().as_ref(),
    ])
    .expect("缺失文件仍应先解析为 Present CLI 调用");

    let output = run_invocation(invocation);
    let json: serde_json::Value =
        serde_json::from_str(&output.stdout).expect("本地预检错误 stdout 必须是 JSON");

    assert_eq!(
        json["ok"], false,
        "缺失 Preview 的协议输出必须显式标记 ok=false"
    );
    assert_eq!(
        json["error"]["code"], "preview_not_found",
        "缺失文件必须返回稳定 preview_not_found"
    );
    assert_eq!(
        output.exit_code, 5,
        "缺失 Preview 是确定性请求错误，退出码必须为 5"
    );
    assert!(
        output.stderr.is_empty(),
        "默认 JSON 错误不得把普通诊断混入 stderr"
    );
    assert_ne!(
        json["error"]["code"], "app_unavailable",
        "本地路径预检必须先于 App socket 连接"
    );
    assert_ne!(
        json["error"]["code"], "internal_error",
        "缺失路径不得退化为 internal_error"
    );
}

/// Preview 准备边界：相对路径应 canonicalize，可读文件保留，目录与未知类型应拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   临时根目录 = 1
///   文件数 = 2：report.md + report.bin
///   目录数 = 1：nested
///   report.md bytes = 8（"# Report"）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用相对路径准备 Markdown 文件。
///   2. 尝试准备目录和无显式类型的 `.bin` 文件。
///   3. 再以显式 Text 类型准备同一个 `.bin` 文件。
///
/// 预期结果：
///   - 正断言：Markdown 与显式 Text 都返回绝对 canonical path。
///   - 正断言：目录返回 PreviewNotFile，未知扩展返回 UnsupportedPreviewType。
///   - 负断言：成功结果不得保留相对路径。
#[test]
fn preview_preparation_canonicalizes_files_and_rejects_directories_or_unknown_types() {
    let temp = tempfile::tempdir().expect("测试应创建隔离 Preview 根目录");
    let markdown_path = temp.path().join("report.md");
    let unknown_path = temp.path().join("report.bin");
    let directory_path = temp.path().join("nested");
    fs::write(&markdown_path, "# Report").expect("测试应写入 Markdown 文件");
    fs::write(&unknown_path, b"plain").expect("测试应写入未知扩展文件");
    fs::create_dir(&directory_path).expect("测试应创建目录目标");

    let markdown = prepare_presentation_target(PresentationTarget::Preview {
        path: markdown_path.clone(),
        explicit_type: None,
    })
    .expect("可读 Markdown 文件应准备成功");
    let directory = prepare_presentation_target(PresentationTarget::Preview {
        path: directory_path,
        explicit_type: None,
    });
    let unknown = prepare_presentation_target(PresentationTarget::Preview {
        path: unknown_path.clone(),
        explicit_type: None,
    });
    let explicit_text = prepare_presentation_target(PresentationTarget::Preview {
        path: unknown_path,
        explicit_type: Some(PreviewType::Text),
    })
    .expect("显式 Text 应允许未知扩展的可读文件");

    let expected_markdown =
        fs::canonicalize(&markdown_path).expect("测试 Markdown 路径应可 canonicalize");
    assert_eq!(
        markdown,
        PresentationTarget::Preview {
            path: expected_markdown.clone(),
            explicit_type: None,
        },
        "Preview 成功结果必须使用文件系统 canonical path"
    );
    assert_eq!(
        directory,
        Err(PresentationPreparationError::PreviewNotFile),
        "目录不能作为只读 Preview 文件展示"
    );
    assert_eq!(
        unknown,
        Err(PresentationPreparationError::UnsupportedPreviewType),
        "无显式类型的未知扩展必须稳定拒绝"
    );
    assert_eq!(
        explicit_text,
        PresentationTarget::Preview {
            path: fs::canonicalize(temp.path().join("report.bin"))
                .expect("显式 Text 测试文件应可 canonicalize"),
            explicit_type: Some(PreviewType::Text),
        },
        "显式 Text 类型必须保留并返回 canonical path"
    );
    let PresentationTarget::Preview { path, .. } = markdown else {
        panic!("Markdown 准备成功后必须仍为 Preview target");
    };
    assert!(
        path.is_absolute(),
        "Preview 准备成功后不得把相对路径发送给 App"
    );
}

/// Present 协议输出：成功结果应扁平化，展示错误应保留专用稳定错误码。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 41
///   kind = Browser；disposition = Reused；layout = Master
///   error = forbidden_url_scheme；退出码期望 = 5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 渲染 Presented 成功响应。
///   2. 渲染 Presentation 专用失败响应。
///   3. 解析两份 stdout 并验证稳定字段。
///
/// 预期结果：
///   - 正断言：成功 result 直接包含 node_id/kind/disposition/layout。
///   - 正断言：失败 code=forbidden_url_scheme 且退出码为确定性请求错误 5。
///   - 负断言：Presentation 错误不得退化为 internal_error。
#[test]
fn presentation_protocol_output_is_flat_and_preserves_stable_error_codes() {
    let success = render_protocol_success(
        OutputFormat::Json,
        "present-41".to_string(),
        &CardCommandResult::Presented {
            node_id: 41,
            kind: PresentationKind::Browser,
            disposition: PresentationDisposition::Reused,
            layout: PresentationLayout::Master,
        },
    )
    .expect("Presented 结果应可序列化");
    let failure = render_protocol_error(
        OutputFormat::Json,
        "present-error".to_string(),
        &CardCommandError::presentation(
            PresentationErrorCode::ForbiddenUrlScheme,
            "data scheme is not allowed",
            false,
        ),
    )
    .expect("Presentation 错误应可序列化");
    let success_json: serde_json::Value =
        serde_json::from_str(&success.stdout).expect("Present 成功 stdout 必须是 JSON");
    let failure_json: serde_json::Value =
        serde_json::from_str(&failure.stdout).expect("Present 失败 stdout 必须是 JSON");

    assert_eq!(
        success_json["result"]["node_id"], 41,
        "Present 成功必须直接返回可继续定位的 node_id"
    );
    assert_eq!(
        success_json["result"]["kind"], "browser",
        "Present 成功必须以 snake_case 输出目标类型"
    );
    assert_eq!(
        success_json["result"]["disposition"], "reused",
        "复用结果必须如实输出 reused"
    );
    assert_eq!(
        success_json["result"]["layout"], "master",
        "Present 成功必须确认 Master Layout 已应用"
    );
    assert_eq!(
        failure_json["error"]["code"], "forbidden_url_scheme",
        "危险 scheme 错误必须保留稳定专用 code"
    );
    assert_eq!(
        failure.exit_code, 5,
        "确定性 Presentation 请求错误必须使用退出码 5"
    );
    assert_ne!(
        failure_json["error"]["code"], "internal_error",
        "Presentation 校验错误不得退化为 internal_error"
    );
}
