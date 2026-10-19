//! Explicit opt-in cloud checks. Credentials are read from a private file, never printed.
use live_voice::{connect, AppendKind, LiveCommand, LiveConfig, LiveEvent};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

/// AC15/AC18：真实服务接受启动并在持续静音上行期间生成PCM，关闭后返回最终用量。
/// 数据构造：从仓库外的私有文件读Key，固定16kHz/20ms静音，不读取真实麦克风。
/// 执行过程：连接gpt-live-1，要求中文问候，排空下行10秒，然后优雅关闭。
/// 预期结果：收到非零PCM、助手字幕及最终累计用量；它不证明物理设备或用户插话。
#[tokio::test]
#[ignore = "requires explicit paid cloud access and NORMA_LIVE_TEST_KEY_FILE"]
async fn real_live_greeting_and_final_usage() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let path = PathBuf::from(std::env::var_os("NORMA_LIVE_TEST_KEY_FILE")
            .expect("必须显式提供私有凭据文件路径"));
        let key = std::fs::read_to_string(path).expect("私有凭据文件应可读");
        let config = LiveConfig {
            instructions: "你是Norma。请用中文简短对话，只做语音连通性测试。".into(),
            ..Default::default()
        };
        let mut connection = connect(config, key.trim(), CancellationToken::new())
            .await.expect("真实Live会话应通过认证和session.started屏障");
        println!("session_id={}",connection.session_id());
        drop(key);
        let control = connection.control();
        control.send_command(&LiveCommand::Append {
            event_id: "cloud-smoke-greeting".into(),
            kind: AppendKind::Instructions,
            delegation_id: None,
            content: "现在用中文说：你好，实时语音连接成功。然后停下来听。".into(),
        }).expect("应能提交问候指令");
        let mut clock = tokio::time::interval(Duration::from_millis(20));
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let deadline = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(deadline);
        let mut audio_bytes = 0usize;
        let mut nonzero = false;
        let mut assistant_text = String::new();
        loop {
            tokio::select! {
                biased;
                _ = &mut deadline => break,
                _ = clock.tick() => control.send_silence(320).expect("持续静音上行应保持有界"),
                event = connection.next_event() => match event {
                    Some(LiveEvent::OutputAudio(pcm)) => {
                        audio_bytes += pcm.bytes.len();
                        nonzero |= pcm.bytes.iter().any(|byte| *byte != 0);
                    }
                    Some(LiveEvent::Transcript(part)) if part.speaker == live_voice::Speaker::Assistant => assistant_text.push_str(&part.delta),
                    Some(LiveEvent::Rejected { code, .. }) => panic!("云端拒绝控制命令: {code:?}"),
                    Some(LiveEvent::Closed { reason, .. }) => panic!("会话提前关闭: {reason}"),
                    None => panic!("云端事件流提前结束"),
                    _ => {},
                }
            }
        }
        let outcome = connection.finish().await;
        println!("cloud smoke: audio_bytes={audio_bytes}, nonzero={nonzero}, assistant_chars={}, seconds={}, finalized={}, error={:?}",
            assistant_text.chars().count(), outcome.usage.seconds, outcome.usage.finalized, outcome.error);
        assert!(audio_bytes > 0 && nonzero, "必须收到真实非零输出PCM");
        assert!(!assistant_text.is_empty(), "必须收到助手字幕");
        assert!(outcome.error.is_none(), "真实正常关闭不应报错");
        assert!(outcome.usage.finalized, "最终用量必须由session.closed确认");
        assert!(outcome.usage.seconds >= 9.0, "10秒测试应有相应的服务端累计时长");
    }).await.expect("云端检查必须在30秒内结束并释放连接");
}

/// AC18局部：真实服务连续10分钟，独立mute/unmute后挂断并再建两次连接。
/// 数据构造：私有Key和20ms零PCM，不启动设备，也不创建项目副作用。
/// 执行过程：首会话600秒，40秒mute、50秒unmute；其余各1秒并优雅关闭。
/// 预期结果：3个不同session，队列有界、控制ACK与最终用量均确认；设备稳态另验。
#[tokio::test]
#[ignore = "explicit paid ten-minute cloud stability check"]
async fn real_live_ten_minutes_and_three_sessions() {
    tokio::time::timeout(Duration::from_secs(660), async {
        let key = std::fs::read_to_string(std::env::var_os("NORMA_LIVE_TEST_KEY_FILE").expect("私有Key路径"))
            .expect("私有Key可读");
        let mut sessions = std::collections::HashSet::new();
        for index in 0..3 {
            let mut connection = connect(LiveConfig {
                instructions:"这是静音稳定性测试，请保持安静。".into(), ..Default::default()
            }, key.trim(), CancellationToken::new()).await.expect("每次真实会话都应通过started屏障");
            println!("stability session={index}, id={}",connection.session_id());
            assert!(sessions.insert(connection.session_id().to_owned()), "每次重开应使用新session");
            let control = connection.control();
            let origin = tokio::time::Instant::now();
            let seconds = if index == 0 {600} else {1};
            let mut clock = tokio::time::interval(Duration::from_millis(20));
            clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut muted = false;
            let mut unmuted = false;
            let mut mute_acks = 0;
            let mut max_queue = 0;
            loop {
                tokio::select! {
                    biased;
                    _ = clock.tick() => {
                        if origin.elapsed() >= Duration::from_secs(seconds) {break;}
                        if index == 0 && !muted && origin.elapsed() >= Duration::from_secs(40) {
                            control.send_command(&LiveCommand::Mute{event_id:"stability-mute".into(),muted:true}).expect("mute命令");
                            muted = true;
                        }
                        if index == 0 && !unmuted && origin.elapsed() >= Duration::from_secs(50) {
                            control.send_command(&LiveCommand::Mute{event_id:"stability-unmute".into(),muted:false}).expect("unmute命令");
                            unmuted = true;
                        }
                        control.send_silence(320).expect("十分钟连续静音应保持有界");
                        max_queue = max_queue.max(control.queued_audio_bytes());
                    }
                    event = connection.next_event() => match event {
                        Some(LiveEvent::Acknowledged{kind:live_voice::Acknowledgement::InputMuted | live_voice::Acknowledgement::InputUnmuted,..}) => mute_acks += 1,
                        Some(LiveEvent::Rejected{code,..}) => panic!("服务拒绝: {code:?}"),
                        Some(LiveEvent::Closed{reason,..}) => panic!("服务提前关闭: {reason}"),
                        None => panic!("十分钟内连接意外结束"),
                        _ => {},
                    }
                }
            }
            let outcome = connection.finish().await;
            println!("stability end={index}, seconds={}, finalized={}, max_input_queue_bytes={max_queue}, mute_acks={mute_acks}, error={:?}",outcome.usage.seconds,outcome.usage.finalized,outcome.error);
            assert!(outcome.error.is_none() && outcome.usage.finalized, "正常结束必须确认用量");
            assert!(max_queue <= 32000, "上行积压不得超过1秒PCM预算");
            if index == 0 {
                assert!(muted && unmuted && mute_acks == 2, "真实mute/unmute均需ACK");
                assert!(outcome.usage.seconds >= 599.0, "必须覆盖完整10分钟活跃会话");
            }
        }
    }).await.expect("稳定性测试必须在11分钟内释放连接");
}
