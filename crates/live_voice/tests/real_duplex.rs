use audio::{PcmAudioChunk, PcmFormat};
use live_voice::{connect, AppendKind, LiveCommand, LiveConfig, LiveEvent, Speaker};
use std::{path::PathBuf, time::Duration};
use tokio_util::sync::CancellationToken;

/// AC15/AC17局部：真实中文数字注入、client delegation与说话期间的第二段输入。
/// 数据构造：仓库外的16k mono PCM固定中文请求和“停一下”夹具；无物理设备。
/// 执行过程：20ms连续流→请求建终端→输出时追加纠正→如实回传未执行→关闭。
/// 预期结果：双向字幕、真实委派、输出期间输入不暂停；不把数字注入算外放验收。
#[tokio::test]
#[ignore = "requires explicit paid cloud access and private PCM fixtures"]
async fn real_live_input_delegation_and_overlapping_correction() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let key_path = std::env::var_os("NORMA_LIVE_TEST_KEY_FILE").expect("私有Key文件路径");
        let fixtures = PathBuf::from(std::env::var_os("NORMA_LIVE_TEST_AUDIO_DIR").expect("固定PCM夹具目录"));
        let request = std::fs::read(fixtures.join("request.pcm")).expect("请求PCM");
        let interrupt = std::fs::read(fixtures.join("interrupt.pcm")).expect("插话PCM");
        let key = std::fs::read_to_string(key_path).expect("私有Key可读");
        let mut connection = connect(LiveConfig {
            instructions: "你是Norma，用中文。先安静听用户。项目操作必须委派给client，未收到真实成功回执不得声称已执行。用户请求创建终端时先委派，然后慢慢从一数到十以测试双工；用户说停一下时停止数数并听新要求。".into(),
            ..Default::default()
        }, key.trim(), CancellationToken::new()).await.expect("真实Live启动");
        println!("session_id={}", connection.session_id());
        drop(key);
        let control = connection.control();
        let origin = tokio::time::Instant::now();
        let mut clock = tokio::time::interval(Duration::from_millis(20));
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut request_cursor = 0;
        let mut interrupt_cursor = 0;
        let mut correcting = false;
        let mut last_output = None;
        let mut overlapping_frames = 0;
        let mut audio_bytes = 0;
        let mut delegations = 0;
        let mut user_text = String::new();
        let mut assistant_text = String::new();
        loop {
            tokio::select! {
                biased;
                _ = clock.tick() => {
                    if origin.elapsed() >= Duration::from_secs(25) { break; }
                    let mut bytes = vec![0;640];
                    let (source, cursor) = if correcting { (&interrupt, &mut interrupt_cursor) }
                        else { (&request, &mut request_cursor) };
                    if origin.elapsed() >= Duration::from_secs(1) && *cursor < source.len() {
                        let count = (source.len()-*cursor).min(640);
                        bytes[..count].copy_from_slice(&source[*cursor..*cursor+count]);
                        *cursor += count;
                        if correcting && bytes.iter().any(|byte| *byte != 0)
                            && last_output.is_some_and(|at: tokio::time::Instant| at.elapsed() <= Duration::from_millis(200)) {
                            overlapping_frames += 1;
                        }
                    }
                    control.send_audio(PcmAudioChunk::new(PcmFormat::Pcm16,16000,1,bytes).expect("20msPCM"))
                        .expect("持续输入应有界");
                }
                event = connection.next_event() => match event {
                    Some(LiveEvent::OutputAudio(pcm)) => {
                        audio_bytes += pcm.bytes.len();
                        if pcm.bytes.iter().any(|byte| *byte != 0) {
                            last_output = Some(tokio::time::Instant::now());
                            if request_cursor == request.len() { correcting = true; }
                        }
                    }
                    Some(LiveEvent::Transcript(part)) => {
                        println!("{:?} [{},{}] local_ms={}: {}",part.speaker,part.start_ms,part.end_ms,origin.elapsed().as_millis(),part.delta);
                        match part.speaker { Speaker::User => user_text.push_str(&part.delta), Speaker::Assistant => assistant_text.push_str(&part.delta) }
                    }
                    Some(LiveEvent::Delegation(notice)) => {
                        delegations += 1;
                        println!("delegation offset_ms={}, local_ms={}",notice.offset_ms,origin.elapsed().as_millis());
                        control.send_command(&LiveCommand::Append {
                            event_id:format!("fixture-result-{delegations}"),kind:AppendKind::Thinking,
                            delegation_id:Some(notice.id),content:"这只是语音测试，宿主未创建任何卡片，也没有执行任何项目操作。".into(),
                        }).expect("真实委派应可关联事实更新");
                    }
                    Some(LiveEvent::Rejected {code,..}) => panic!("云端拒绝: {code:?}"),
                    Some(LiveEvent::Closed {reason,..}) => panic!("提前关闭: {reason}"),
                    None => panic!("事件流提前结束"),
                    _ => {},
                }
            }
        }
        let outcome = connection.finish().await;
        println!("duplex: sent_request={request_cursor}, sent_correction={interrupt_cursor}, overlap_frames={overlapping_frames}, audio_bytes={audio_bytes}, delegations={delegations}, seconds={}, finalized={}, error={:?}",outcome.usage.seconds,outcome.usage.finalized,outcome.error);
        assert_eq!(request_cursor, request.len(), "第一段输入必须完整发送");
        assert_eq!(interrupt_cursor, interrupt.len(), "输出后必须持续发送完整插话");
        assert!(overlapping_frames > 0, "助手输出不能阻止第二段输入");
        assert!(user_text.contains("终端") && user_text.contains("苹果"), "实际输入字幕应覆盖请求与纠正");
        assert!(!assistant_text.is_empty() && audio_bytes > 0, "必须有真实音频和输出字幕");
        assert!(delegations > 0, "项目操作请求必须产生client委派");
        assert!(outcome.error.is_none() && outcome.usage.finalized, "真实会话应正常关闭并确认用量");
    }).await.expect("数字双工测试应在45秒内释放资源");
}
