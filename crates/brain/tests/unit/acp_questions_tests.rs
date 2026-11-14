use super::*;

/// 场景：两种真实适配器的原生form保持题目与选项，自定义入口不重复。
/// 数据构造：已抓取的Claude/Codex请求原文；执行：解析params；预期：一题、两个有说明的选项并允许自定义。
#[test]
fn captured_native_forms_preserve_question_semantics() {
    for fixture in [
        include_str!("../fixtures/staff/claude-question.json"),
        include_str!("../fixtures/staff/codex-question.json"),
    ] {
        let value: Value = serde_json::from_str(fixture).expect("fixture JSON");
        let items = parse_form(&value["params"]).expect("native form");
        assert_eq!(items.len(), 1, "伴随自定义字段不能变成第二题");
        assert_eq!(
            items[0].prompt, "Which interface theme do you prefer?",
            "保留完整题目而非短标题"
        );
        assert_eq!(items[0].options.len(), 2, "自定义入口不能重复成为选项");
        assert!(
            items[0].allow_custom && !items[0].multiple && !items[0].secret,
            "单选及自定义语义准确"
        );
        assert!(
            items[0]
                .options
                .iter()
                .all(|option| option.description.is_some()),
            "选项说明保留"
        );
    }
}

/// 场景：普通MCP授权form不能成为提问卡；构造带token字段的form；执行解析；预期拒绝识别。
#[test]
fn generic_elicitation_is_not_a_staff_question() {
    let params = json!({"mode":"form","toolCallId":"mcp-login","message":"Authorize",
        "requestedSchema":{"properties":{"token":{"type":"string","title":"Token"}}}});
    assert!(parse_form(&params).is_none(), "不接入权限或未知MCP表单");
}

/// 场景：到期或会话结束都必须收口待答卡，不能留下永久需要你。
/// 数据构造：持久任务和真实stdio连接，打开一题后将内部单调时钟截止点推进到现在。
/// 执行过程：分别执行超时poll及会话结束poll；预期：卡片TimedOut、无挂起请求、迟到回答不生效。
#[test]
fn expiration_and_session_end_settle_durable_cards() {
    for expired in [true, false] {
        let root = tempfile::tempdir().expect("root");
        let store = Arc::new(TaskStore::new(root.path().to_owned()));
        store
            .create(norma::protocol::Task::new("t", "w", "p", "Theme"))
            .expect("create");
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut host = QuestionHost::new(
            store.clone(),
            BrainScope {
                workspace_id: "w".into(),
                route_id: "r".into(),
            },
            "t".into(),
            1,
            tx,
        );
        let value: Value =
            serde_json::from_str(include_str!("../fixtures/staff/claude-question.json"))
                .expect("fixture");
        assert!(
            host.open(&json!(42), &value["params"]).expect("open"),
            "真实请求必须打开"
        );
        assert!(host.waiting(), "回答前保留挂起请求");
        if expired {
            host.pending[0].deadline = Instant::now();
        }
        let mut conn = Connection::spawn(
            std::path::Path::new("/bin/cat"),
            &[],
            root.path(),
            &[],
            crate::backend::BackendKind::OpenCode,
        )
        .expect("stdio");
        host.poll(&mut conn, !expired).expect("settle");
        assert!(!host.waiting(), "收口后没有悬挂请求");
        let task = store.get("w", "t").expect("task");
        let TaskMessage::Question { question } = &task.messages[0] else {
            panic!("question card")
        };
        assert_eq!(
            question.state,
            StaffQuestionState::TimedOut,
            "状态必须持久化为超时"
        );
        let (_, claimed) = store
            .resolve_question("w", "t", &question.id, StaffQuestionState::Answered, &[])
            .expect("late");
        assert!(!claimed, "终态拒绝迟到回答");
    }
}

/// 场景：Codex无选项的自由输入允许isOther=false，密码题同样需要能输入。
/// 数据构造：安装版适配器产生的无oneOf string字段，分别secret=false/true。
/// 执行：解析原生form；预期：题目被接收、可自由填写，secret语义保留而非返回unsupported。
#[test]
fn codex_text_only_questions_allow_input_without_is_other() {
    for secret in [false, true] {
        let params = json!({"mode":"form","toolCallId":"native-input","requestedSchema":{"properties":{
            "name":{"type":"string","title":"Build name?","_meta":{"codex":{"isOther":false,"isSecret":secret}}}
        }}});
        let items = parse_form(&params).expect("native text input");
        assert!(items[0].allow_custom, "无选项题必须有输入路径");
        assert!(items[0].options.is_empty(), "不能凭空制造选项");
        assert_eq!(items[0].secret, secret, "密码属性不能丢失");
    }
}
