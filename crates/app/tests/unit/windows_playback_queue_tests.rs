use super::{Pending, Playback};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::{mpsc, Arc, Mutex};

#[derive(Default)]
struct DeviceState {
    written: u64,
    padding: u32,
    bytes: Vec<u8>,
    stops: usize,
}
struct ControlledDevice(Arc<Mutex<DeviceState>>);
impl super::OutputDevice for ControlledDevice {
    fn rate(&self) -> u32 {
        16_000
    }
    fn capacity(&self) -> u32 {
        160
    }
    fn frames_written(&self) -> u64 {
        self.0.lock().expect("受控设备锁应有效").written
    }
    fn padding(&self) -> Result<u32, String> {
        Ok(self.0.lock().expect("受控设备锁应有效").padding)
    }
    fn write(&mut self, bytes: &[u8], frames: u32) -> Result<(), String> {
        let mut state = self.0.lock().expect("受控设备锁应有效");
        state.written += u64::from(frames);
        state.padding += frames;
        state.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn stop(&mut self) -> Result<(), String> {
        self.0.lock().expect("受控设备锁应有效").stops += 1;
        Ok(())
    }
}

/// 场景：等待ACK超时但接收端尚未析构时，迟到ACK发送仍成功，不能因此播放失败请求。
/// 构造：真实ACK通道先超时且保持receiver存活，生产Playback队列和可见写入的受控设备。
/// 执行：迟到ACK成功发送后推进生产tick；预期没有任何PCM写入或启动/完成信号。
/// 该受控端点只证明软件协议，不能作为物理扬声器验收。
#[test]
fn a_late_ack_does_not_authorize_audio_after_the_waiter_timed_out() {
    let state = Arc::new(Mutex::new(DeviceState::default()));
    let mut playback = Playback {
        device: Some(Box::new(ControlledDevice(Arc::clone(&state)))),
        ..Default::default()
    };
    let (mut pending, starts, completions) = pending(1, 3);
    pending.offset = 0;
    pending.first_frame = None;
    pending.last_frame = None;
    playback
        .enqueue(pending)
        .expect("受控设备应能接收尚未确认的调度请求");
    let (ack, waiter) = mpsc::channel::<Result<(), String>>();
    assert!(
        matches!(
            waiter.recv_timeout(std::time::Duration::from_millis(1)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "必须先实际见证ACK等待超时"
    );
    ack.send(Ok(()))
        .expect("receiver尚未析构，迟到ACK确实会发送成功");
    playback.tick().expect("生产tick应安全处理未确认请求");
    assert!(
        state.lock().expect("受控设备锁应有效").bytes.is_empty(),
        "ACK发送成功不等于调用方收到确认，不能写入音频"
    );
    assert!(
        starts.is_empty() && completions.is_empty(),
        "超时的调度不能发出播放成功证据"
    );
}

/// 场景：首帧在两个tick之间已经消费，Stop仍必须保留已实际发声的启动见证。
/// 构造：三个已提交帧，padding=2表示仅首帧已消费，尚未执行下一次tick。
/// 执行：生产stop；预期先冻结读取最终消费游标，发送启动，但未消费尾帧不得完成。
#[test]
fn stop_preserves_a_start_that_happened_between_ticks() {
    let state = Arc::new(Mutex::new(DeviceState {
        written: 3,
        padding: 2,
        ..Default::default()
    }));
    let (pending, starts, completions) = pending(1, 3);
    let mut playback = Playback {
        device: Some(Box::new(ControlledDevice(Arc::clone(&state)))),
        pending: [pending].into(),
        bytes: 6,
        ..Default::default()
    };
    playback.stop().expect("受控设备停止应成功");
    starts
        .try_recv()
        .expect("Stop之前已消费首帧，必须保留启动见证");
    assert!(
        completions.is_empty() && completions.is_closed(),
        "未消费尾帧只能取消，不能假报完成"
    );
    assert_eq!(
        state.lock().expect("受控设备锁应有效").stops,
        1,
        "停止确认必须实际调用原生端点适配器"
    );
}

fn pending(
    first: u64,
    last: u64,
) -> (
    Pending,
    async_channel::Receiver<()>,
    async_channel::Receiver<()>,
) {
    let (started, starts) = async_channel::bounded(1);
    let (completed, completions) = async_channel::bounded(1);
    let frames = (last - first + 1) as usize;
    let pending = Pending {
        chunk: PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; frames * 2])
            .expect("帧数推导的PCM16测试块应合法"),
        offset: frames * 2,
        first_frame: Some(first),
        last_frame: Some(last),
        started: Some(started),
        completed,
    };
    (pending, starts, completions)
}

/// 场景：完成信号只能随真实消费游标推进，不能随调度或理论时长发出。
/// 构造：前一个已播放帧后，第一块占2..4三帧、第二块占5..6两帧，共10字节。
/// 执行：调用WASAPI tick实际使用的消费收口逻辑，将游标依次推进至1、2、4、5、6。
/// 预期：按首帧/尾帧分别发一次启动与完成，延迟的第二块不能提前完成，内存预算逐块释放。
#[test]
fn completion_follows_the_native_consumption_cursor() {
    let (first, start_a, done_a) = pending(2, 4);
    let (second, start_b, done_b) = pending(5, 6);
    let mut playback = Playback {
        pending: [first, second].into(),
        bytes: 10,
        ..Default::default()
    };
    playback.retire_consumed(1);
    assert!(
        start_a.is_empty() && done_a.is_empty(),
        "第一块首帧尚未消费，不能启动或完成"
    );
    playback.retire_consumed(2);
    start_a.try_recv().expect("首帧消费后应准确报告第一块启动");
    assert!(
        done_a.is_empty() && start_b.is_empty(),
        "第一块未结束，第二块也不能提前启动"
    );
    playback.retire_consumed(4);
    done_a.try_recv().expect("第一块最后一帧消费后才可报告完成");
    assert_eq!(playback.bytes, 4, "第一块6字节释放后只能保留第二块4字节");
    assert!(
        done_b.is_empty() && start_b.is_empty(),
        "第二块的5..6帧尚未消费，不能假报启动或完成"
    );
    playback.retire_consumed(5);
    start_b.try_recv().expect("第二块首帧消费后才可报告启动");
    assert!(done_b.is_empty(), "第二块仍有一帧未播放，不能完成");
    playback.retire_consumed(6);
    done_b.try_recv().expect("第二块最后一帧消费后才可完成");
    assert_eq!(playback.bytes, 0, "全部播放完成后必须释放所有PCM预算");
    assert!(playback.pending.is_empty(), "全部完成后不能遗留PCM块");
    playback.retire_consumed(6);
    assert!(
        done_a.try_recv().is_err() && done_b.try_recv().is_err(),
        "重复游标不能重复发出完成事件"
    );
}

/// 场景：停止或调度超时不能产生伪完成，也不能保留迟到的PCM。
/// 构造：三帧待播块；执行真实未确认调度删除及stop队列清理路径。
/// 预期：通知通道关闭而非收到成功，PCM预算精确归零。
#[test]
fn stopping_and_timed_out_scheduling_never_complete_pending_pcm() {
    for timed_out in [false, true] {
        let (pending, starts, completions) = pending(1, 3);
        let mut playback = Playback {
            pending: [pending].into(),
            bytes: 6,
            ..Default::default()
        };
        if timed_out {
            playback.discard_last();
        } else {
            playback.stop().expect("无原生设备的队列停止应成功");
        }
        assert_eq!(playback.bytes, 0, "停止或超时都必须释放六字节PCM预算");
        assert!(playback.pending.is_empty(), "停止或超时不能留下迟到播放块");
        assert!(
            starts.is_closed() && starts.is_empty(),
            "取消的待播块不能产生伪启动"
        );
        assert!(
            completions.is_closed() && completions.is_empty(),
            "取消的待播块不能产生伪完成"
        );
    }
}
