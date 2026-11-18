use brain::staff::naming::{named_in, user_words, NameRef};
use brain::staff::TaskStore;
use norma::protocol::{
    compose_image_message, QuestionItem, QuestionOption, ReplyStatus, ReplyTrigger, StaffOp,
    StaffQuestion, Task, TaskMessage,
};

fn grace() -> NameRef<'static> {
    NameRef {
        name: "Grace",
        speech_alias: None,
    }
}

fn store() -> (tempfile::TempDir, TaskStore) {
    let dir = tempfile::tempdir().expect("root");
    let store = TaskStore::new(dir.path().to_owned());
    store
        .create(Task::new("a", "ws", "project", "Ship"))
        .expect("create");
    (dir, store)
}

fn say(store: &TaskStore, text: &str) {
    store
        .execute(
            "ws",
            StaffOp::Message {
                task_id: "a".into(),
                text: text.into(),
            },
        )
        .expect("message");
}

/// 场景：点名只认名册人名和语音别名的整词，不把英文短名嵌进别的单词。
///
/// 数据构造：Grace 没有别名；小王只作为语音别名；Ed 是两个字符的英文名。
/// 执行过程：分别用三句原话调用 named_in。
/// 预期结果：
///   - 正断言：「让 Grace 合并」「grace, merge it」「让小王删一下」命中
///   - 负断言：Ed 不命中 merged；「用 claude 做」不命中 Claude 这个回退称呼
#[test]
fn names_match_whole_words_and_chinese_aliases() {
    assert!(
        named_in("让 Grace 合并", grace()),
        "中文句子里的英文名按单词边界命中"
    );
    assert!(
        named_in("grace, merge it", grace()),
        "逗号是边界，大小写不敏感"
    );
    assert!(
        named_in(
            "让小王删一下",
            NameRef {
                name: "Wang",
                speech_alias: Some("小王"),
            }
        ),
        "语音别名整段包含即算点名"
    );
    assert!(
        !named_in(
            "merged",
            NameRef {
                name: "Ed",
                speech_alias: None,
            }
        ),
        "Ed 嵌在 merged 里不是点名"
    );
    assert!(
        !named_in(
            "用 claude 做",
            NameRef {
                name: "Claude",
                speech_alias: None,
            }
        ),
        "引擎 slug 和同形回退名不参与点名"
    );
    assert!(
        !named_in(
            "让 A 做",
            NameRef {
                name: "A",
                speech_alias: None,
            }
        ),
        "少于 2 个字符的名字不参与"
    );
}

/// 场景：本轮原话只取上一轮结束后的用户消息，图片路径和回答选项都不能借来授权。
///
/// 数据构造：
///   第一轮用户说「让 Grace 合并」，幕僚回复结束后再进入第二轮。
///   第二轮只有一条回答选项，选项文字是「让 Grace 合并」，另有一条图片消息，
///   可见文字是「看一下」，路径里带 Grace。图片哈希是 64 位小写十六进制，协议要求的长度。
/// 执行过程：第一轮 begin_turn 记下原话；settle 后写入 Answered 和图片消息，再 begin_turn。
/// 预期结果：
///   - 正断言：第一轮原话是「让 Grace 合并」，并且 named_in 为真
///   - 负断言：第二轮原话只有「看一下」，不含 Grace，named_in 为假
#[test]
fn user_words_drop_prior_rounds_answers_and_image_paths() {
    let (_dir, store) = store();
    say(&store, "让 Grace 合并");
    let first = store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("first turn");
    assert_eq!(
        first.user_words, "让 Grace 合并",
        "本轮开始前的用户原话要留下来"
    );
    assert!(named_in(&first.user_words, grace()), "这一轮确实点了 Grace");
    store
        .settle_reply("ws", "a", ReplyStatus::Done, 1, None)
        .expect("settle");
    let hash = "ab".repeat(32);
    let path = format!("/tmp/Grace/.mobile-images/{hash}.png");
    let image =
        compose_image_message("看一下", std::slice::from_ref(&path)).expect("image message");
    assert!(
        image.contains("Grace"),
        "图片路径里确实写着 Grace，用来证明它被剔除"
    );
    store
        .update("ws", "a", |task| {
            task.questions.push(StaffQuestion {
                id: "q".into(),
                items: vec![QuestionItem {
                    key: "pick".into(),
                    provider_id: None,
                    prompt: "交给谁".into(),
                    options: vec![QuestionOption {
                        key: "grace".into(),
                        label: "让 Grace 合并".into(),
                        description: None,
                    }],
                    multiple: false,
                    allow_custom: false,
                    secret: false,
                }],
                ..StaffQuestion::default()
            });
            task.messages.push(TaskMessage::Answered {
                question_id: "q".into(),
            });
            task.messages.push(TaskMessage::User { text: image });
            Ok(())
        })
        .expect("record answer and image");
    let second = store
        .begin_turn("ws", "a", ReplyTrigger::User)
        .expect("second turn");
    assert_eq!(second.user_words, "看一下", "只保留图片协议的可见正文");
    assert!(
        !second.user_words.contains("Grace"),
        "路径、选项文字和上一轮原话都不能留下来"
    );
    assert!(!named_in(&second.user_words, grace()), "这一轮没有点名");
    assert_eq!(
        user_words(&second.messages),
        second.user_words,
        "begin_turn 存下来的原话和纯函数一致"
    );
}
