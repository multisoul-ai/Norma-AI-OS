use super::super::session::{CaptureEvent, CaptureMode};
use super::*;
use audio::{PcmAudioChunk, PcmFormat};
use tokio_util::sync::CancellationToken;

static FIXTURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn pcm(byte: u8) -> PcmAudioChunk {
    // 160 mono samples × 2B = 320B = 10ms at 16kHz.
    PcmAudioChunk::new(PcmFormat::Pcm16, 16000, 1, vec![byte; 320]).expect("10ms PCM")
}

/// AC11/AC12：静音释放旧采集会话，解除静音只转发新会话generation的PCM。
/// 数据构造：voice_input已有的隔离duplex fixture，首会话0x11、新会话0x22，均保持打开。
/// 执行过程：启动→收到Ready/PCM0→mute1→安装新fixture并unmute2→接收新PCM→取消并等待worker。
/// 预期结果：新generation不含旧/静音期间音频；退出后事件通道关闭，不在GPUI线程join。
#[tokio::test]
async fn live_capture_restarts_stream_on_unmute_and_closes_on_cancel() {
    let _fixture = FIXTURE_LOCK.lock().await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let released = voice_input::install_test_duplex_pcm_stream(vec![pcm(0x11)]);
        let (desired, mode) = tokio::sync::watch::channel(CaptureMode {
            generation: 0,
            muted: false,
        });
        let (events, captured) = async_channel::bounded(8);
        let cancellation = CancellationToken::new();
        let worker = tokio::spawn(run_capture(None, mode, events, cancellation.clone()));
        assert!(
            matches!(
                captured.recv().await.expect("初始就绪"),
                CaptureEvent::Ready { generation: 0 }
            ),
            "初始采集就绪"
        );
        match captured.recv().await.expect("首包") {
            CaptureEvent::Pcm { generation, chunk } => {
                assert_eq!(generation, 0, "首会话generation");
                assert_eq!(chunk, pcm(0x11), "旧会话fixture");
            }
            _ => panic!("初始应该得到PCM"),
        }
        released.recv().await.expect("fixture采集已启动");
        desired.send_replace(CaptureMode {
            generation: 1,
            muted: true,
        });
        assert!(
            released.recv().await.is_err(),
            "静音期间必须先释放旧采集资源，不能等到unmute"
        );
        assert!(
            matches!(
                captured.recv().await.expect("旧流关闭确认"),
                CaptureEvent::Stopped { generation: 0 }
            ),
            "旧流尾包排空后确认关闭"
        );
        assert!(
            matches!(
                captured.recv().await.expect("静音代次关闭确认"),
                CaptureEvent::Stopped { generation: 1 }
            ),
            "静音代次不启动硬件，只确认关闭"
        );
        assert!(
            captured.try_recv().is_err(),
            "确认后静音期间不再提供旧PCM或新Ready"
        );
        let reopened = voice_input::install_test_duplex_pcm_stream(vec![pcm(0x22)]);
        desired.send_replace(CaptureMode {
            generation: 2,
            muted: false,
        });
        assert!(
            matches!(
                captured.recv().await.expect("新采集就绪"),
                CaptureEvent::Ready { generation: 2 }
            ),
            "新采集只能在旧采集释放后启动"
        );
        match captured.recv().await.expect("新包") {
            CaptureEvent::Pcm { generation, chunk } => {
                assert_eq!(generation, 2, "只采用新generation");
                assert_eq!(chunk, pcm(0x22), "不能恢复旧/静音音频");
            }
            _ => panic!("新会话应该得到PCM"),
        }
        reopened.recv().await.expect("新fixture已启动");
        cancellation.cancel();
        worker.await.expect("采集worker退出");
        assert!(reopened.recv().await.is_err(), "退出释放新采集资源");
        assert!(captured.recv().await.is_err(), "worker退出后事件通道终止");
    })
    .await
    .expect("采集生命周期有界");
}

/// AC11：采集事件队列满时取消也能退出，不能等UI先消费Ready。
/// 数据构造：一个保持打开的fixture采集，容量1且已填满的事件队列；启动/释放观察通道。
/// 执行过程：等待fixture启动→确认事件队列仍满→取消→等待worker和资源释放。
/// 预期结果：取消不依赖UI消费，fixture资源释放；这里只验证生命周期，不替代物理设备测试。
#[tokio::test]
async fn live_capture_cancel_releases_stream_with_full_event_queue() {
    let _fixture = FIXTURE_LOCK.lock().await;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let lifecycle = voice_input::install_test_duplex_pcm_stream(vec![]);
        let (_desired, mode) = tokio::sync::watch::channel(CaptureMode {
            generation: 0,
            muted: false,
        });
        let (events, captured) = async_channel::bounded(1);
        events
            .send(CaptureEvent::Ready { generation: 99 })
            .await
            .expect("填满队列");
        let cancellation = CancellationToken::new();
        let worker = tokio::spawn(run_capture(None, mode, events, cancellation.clone()));
        lifecycle.recv().await.expect("采集已启动");
        assert!(captured.is_full(), "UI尚未消费任何事件");
        cancellation.cancel();
        worker.await.expect("满队列取消可退出");
        assert!(lifecycle.recv().await.is_err(), "取消已释放采集资源");
        assert!(
            matches!(
                captured.recv().await.expect("原事件"),
                CaptureEvent::Ready { generation: 99 }
            ),
            "没有消费或替换UI事件来绕过背压"
        );
        assert!(captured.recv().await.is_err(), "worker的sender已关闭");
    })
    .await
    .expect("满队列取消有界");
}
