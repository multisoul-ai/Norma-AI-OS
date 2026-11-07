use super::{
    bounded_overview, caption_index, split_sentences, OverviewEntry, StaffChat, StaffRole,
};

/// 场景：用户发话与幕僚回复进入同一份有界对话，汇报请求只放行一次强制朗读。
///
/// 数据构造：先发“补文档”和空白，再请求汇报两次，随后收到回复；再收一条无请求的回复。
/// 预期结果：空白被丢弃；重复请求不叠加；第一条回复 speak=true 并消耗请求，第二条 false；
/// 对话顺序为 用户→幕僚→幕僚。
#[test]
fn replies_are_ordered_and_a_report_request_forces_speech_exactly_once() {
    let mut chat = StaffChat::default();
    assert!(chat.push_user("   ").is_none(), "空白不应入对话");
    chat.push_user(" 补文档 ").expect("有效输入应入对话");
    assert!(chat.request_report(), "第一次汇报请求应被接受");
    assert!(!chat.request_report(), "等待回复期间不得叠加请求");
    let first = chat
        .push_reply("Alan 完成了登录。")
        .expect("非空回复应入对话");
    assert!(first.speak, "汇报请求后的回复必须朗读");
    let second = chat.push_reply("还有一件事").expect("非空回复应入对话");
    assert!(!second.speak, "请求已消耗，后续回复不强制朗读");
    assert!(chat.push_reply("  ").is_none(), "空回复不入对话");
    let roles: Vec<_> = chat.lines().map(|line| line.role).collect();
    assert_eq!(roles, [StaffRole::User, StaffRole::Staff, StaffRole::Staff]);
    assert_eq!(
        chat.lines().next().unwrap().text,
        "补文档",
        "用户文本应去掉首尾空白"
    );
}

/// 场景：对话长度有上限，旧消息被丢弃但 id 单调递增。
///
/// 数据构造：连续 60 条用户消息，上限 40。
/// 预期结果：保留最后 40 条，首条 id=20，末条 id=59。
#[test]
fn chat_keeps_only_the_latest_lines() {
    let mut chat = StaffChat::default();
    for index in 0..60 {
        chat.push_user(&format!("m{index}"));
    }
    let ids: Vec<_> = chat.lines().map(|line| line.id).collect();
    assert_eq!(
        (ids.len(), ids[0], ids[39]),
        (40, 20, 59),
        "应只保留最近 40 条"
    );
}

/// 场景：字幕按句切分，中英文句末标点与换行都分句，标点保留在句内。
///
/// 数据构造：“登录完成了。接下来测试！OK. Next?\n最后一句”。
/// 预期结果：5 句，依次保留各自标点；纯空白返回空。
#[test]
fn sentences_split_on_cjk_and_ascii_terminators() {
    assert_eq!(
        split_sentences("登录完成了。接下来测试！OK. Next?\n最后一句"),
        ["登录完成了。", "接下来测试！", "OK.", "Next?", "最后一句"]
    );
    assert!(split_sentences("  \n ").is_empty(), "纯空白没有字幕");
    assert_eq!(
        split_sentences("3.5 版本"),
        ["3.5 版本"],
        "数字中的小数点不分句"
    );
}

/// 场景：字幕随朗读进度推进，按字数比例分配时长，结束后无字幕。
///
/// 数据构造：两句（含句号 5 字、9 字），每字 200ms，最短 800ms：时长 1000ms + 1800ms。
/// 预期结果：0ms→第 0 句；999ms→第 0 句；1000ms→第 1 句；2999ms→第 1 句；2800ms→None。
#[test]
fn captions_advance_by_sentence_weight_and_end_with_the_speech() {
    let sentences = vec!["一二三四。".to_string(), "一二三四五六七八。".to_string()];
    assert_eq!(caption_index(&sentences, 0), Some(0));
    assert_eq!(caption_index(&sentences, 999), Some(0));
    assert_eq!(caption_index(&sentences, 1000), Some(1));
    assert_eq!(caption_index(&sentences, 2799), Some(1));
    assert_eq!(caption_index(&sentences, 2800), None, "朗读结束后字幕消失");
    assert_eq!(caption_index(&[], 0), None);
}

/// 场景：汇报概览必须有界，超出预算时说明被省略的条数且不截断单条中间的字符。
///
/// 数据构造：30 条“工作空间/项目：状态”，预算 120 字符。
/// 预期结果：总长不超过预算，含“还有 N 项”，列出的行均完整；预算足够时全部列出且无省略提示。
#[test]
fn overview_is_bounded_and_reports_omissions() {
    let entries: Vec<_> = (0..30)
        .map(|index| OverviewEntry {
            workspace: format!("空间{index}"),
            project: "项目".into(),
            line: "运行中，等待审批".into(),
        })
        .collect();
    let text = bounded_overview(&entries, 120);
    assert!(
        text.chars().count() <= 120,
        "超出预算：{}",
        text.chars().count()
    );
    assert!(text.contains("还有"), "应说明省略：{text}");
    assert!(text.lines().next().unwrap().contains("空间0"), "{text}");
    let all = bounded_overview(&entries[..2], 400);
    assert!(!all.contains("还有"), "预算足够不应提示省略：{all}");
    assert_eq!(all.lines().count(), 2);
}
