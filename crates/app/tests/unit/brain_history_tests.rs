/// `history` 的轮数缺省是 6，并限制在 1 到 20；空白关键词视为没有。
///
/// 数据构造：不给、给 0、给 99、给 5，以及空白与正常关键词。
/// 预期结果：
///   - 正断言：6、1、20、5；关键词空白时是 None，正常时保留
///   - 负断言：超出范围的数不会原样传下去
#[test]
fn history_options_have_a_default_and_a_clamp() {
    use super::options;
    assert_eq!(options(None, None).last, 6);
    assert_eq!(options(Some(0), None).last, 1);
    assert_eq!(options(Some(99), None).last, 20);
    assert_eq!(options(Some(5), None).last, 5);
    assert_eq!(options(None, Some("  ".into())).grep, None);
    assert_eq!(
        options(None, Some("error".into())).grep.as_deref(),
        Some("error")
    );
}

fn convo(id: &str) -> norma::mobile::ConversationItem {
    norma::mobile::ConversationItem {
        id: id.into(),
        role: "user".into(),
        text: id.into(),
        details: None,
    }
}

/// Claude 会话按页向后读，直到没有下一页；失败立刻返回；页数有上限。
///
/// 数据构造：三页（1、2 / 3 / 4）；一个永远有下一页的读取；第二页就失败的读取。
/// 预期结果：
///   - 正断言：三页读完得到 4 条且顺序不变；永远有下一页时停在 MAX_PAGES 页；失败时返回那条原因
///   - 负断言：失败后不再继续读
#[test]
fn collect_pages_follows_cursors_and_stops() {
    use super::{collect_pages, Page};
    let pages = vec![
        Page {
            items: vec![convo("1"), convo("2")],
            next: Some(1),
        },
        Page {
            items: vec![convo("3")],
            next: Some(2),
        },
        Page {
            items: vec![convo("4")],
            next: None,
        },
    ];
    let mut queue = pages.into_iter();
    let all = collect_pages(|_| Ok(queue.next().expect("不该多读"))).unwrap();
    let ids: Vec<&str> = all.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["1", "2", "3", "4"]);

    let mut calls = 0;
    let capped = collect_pages(|_| {
        calls += 1;
        Ok(Page {
            items: vec![convo(&calls.to_string())],
            next: Some(calls),
        })
    })
    .unwrap();
    assert_eq!(capped.len(), 30, "页数封顶");

    let mut reads = 0;
    let failed = collect_pages(|_: Option<u32>| {
        reads += 1;
        if reads == 2 {
            return Err("坏了".to_string());
        }
        Ok(Page {
            items: vec![convo("x")],
            next: Some(1),
        })
    });
    assert_eq!(failed.unwrap_err(), "坏了");
    assert_eq!(reads, 2, "失败后不再继续读");
}

/// 读取失败的原因要翻成中文说明；认不出的原样带上，不吞掉。
#[test]
fn history_errors_are_explained_in_chinese() {
    use super::explain;
    assert!(explain("Claude has not provided a transcript path").contains("还没有给出会话文件"));
    assert!(explain("controlled Claude run closed").contains("已经结束"));
    assert!(explain("something odd").contains("something odd"));
}

/// 手机端的会话条目原样转成大脑读的条目，工具详情不丢。
#[test]
fn conversation_items_map_to_history_items() {
    use super::to_history_items;
    use norma::mobile::ConversationItem;
    let items = vec![
        ConversationItem {
            id: "1".into(),
            role: "user".into(),
            text: "跑测试".into(),
            details: None,
        },
        ConversationItem {
            id: "2".into(),
            role: "tool".into(),
            text: String::new(),
            details: Some("[{\"type\":\"tool_use\"}]".into()),
        },
    ];
    let mapped = to_history_items(&items);
    assert_eq!(mapped.len(), 2);
    assert_eq!(mapped[0].role, "user");
    assert_eq!(
        mapped[1].details.as_deref(),
        Some("[{\"type\":\"tool_use\"}]")
    );
}
