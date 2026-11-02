use super::{decode_packet, WindowsAudioCapture};
use crate::VoiceInputError;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

/// 场景：WASAPI单声道f32包必须保留样本顺序和数值。
/// 构造：五个已知样本转为原生字节序，容量恰好为五帧。
/// 执行：调用真实采集使用的解包函数；预期五个样本原样输出，无通道/长度偏移。
#[test]
fn capture_packet_preserves_f32_samples() {
    let samples = [-1.0_f32, -0.25, 0.0, 0.5, 1.0];
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_ne_bytes())
        .collect();
    let decoded = decode_packet(Some(&bytes), 5, 5, false).expect("五帧合法包应正确解码");
    assert_eq!(decoded, samples, "采集包必须原样保留所有单声道f32样本");
}

/// 场景：SILENT包允许null数据，非静音包不允许空指针或截断长度。
/// 构造：160帧对应16kHz下10ms，另构造不足两帧的7个字节和超容量帧数。
/// 执行：逐个解包；预期静音精确补零，空包合法，畸形包明确失败。
#[test]
fn capture_packet_handles_silence_and_rejects_invalid_buffers() {
    assert_eq!(
        decode_packet(None, 160, 160, true).expect("静音包可无数据指针"),
        vec![0.0; 160],
        "10ms静音必须输出160个零样本"
    );
    assert_eq!(
        decode_packet(Some(&[255; 8]), 2, 2, true).expect("静音标志优先于内存内容"),
        vec![0.0; 2],
        "静音包不能读取未定义的设备内存"
    );
    assert!(
        decode_packet(None, 0, 160, false)
            .expect("零帧包不需要内存")
            .is_empty(),
        "零帧不能产生假样本"
    );
    for (bytes, frames, capacity, silent) in [
        (None, 2, 2, false),
        (Some([0_u8; 7].as_slice()), 2, 2, false),
        (None, 3, 2, true),
    ] {
        assert_eq!(
            decode_packet(bytes, frames, capacity, silent),
            Err(VoiceInputError::Audio(
                "Windows capture returned an invalid packet".into()
            )),
            "非静音缺数据、截断数据及超容量帧数必须准确拒绝"
        );
    }
}

/// 场景：丢弃采集对象必须停止并join其所有者线程。
/// 构造：真实mpsc停止通道和等待停止信号的线程，线程结束前设置原子见证。
/// 执行：drop真实WindowsAudioCapture所有权包装器；预期返回时线程已经结束。
#[test]
fn dropping_windows_capture_joins_the_worker() {
    let (stop, stopped) = mpsc::channel();
    let joined = Arc::new(AtomicBool::new(false));
    let witness = Arc::clone(&joined);
    let worker = std::thread::spawn(move || {
        stopped.recv().expect("drop必须发送停止信号");
        witness.store(true, Ordering::SeqCst);
        Ok(())
    });
    drop(WindowsAudioCapture {
        stop,
        worker: Some(worker),
    });
    assert!(
        joined.load(Ordering::SeqCst),
        "drop返回前必须完成停止及join，不能泄漏采集线程"
    );
}

/// 场景：驱动在运行期间失败，用户停止时不能得到伪成功。
/// 构造：所有者线程返回确定的设备失效错误；执行真实stop/join路径。
/// 预期：原始错误传回调用方，不能丢弃线程结果或返回Ok。
#[test]
fn stopping_windows_capture_preserves_runtime_failure() {
    let (stop, _stopped) = mpsc::channel();
    let worker = std::thread::spawn(|| Err(VoiceInputError::Audio("device invalidated".into())));
    let capture = WindowsAudioCapture {
        stop,
        worker: Some(worker),
    };
    assert_eq!(
        capture.stop(),
        Err(VoiceInputError::Audio("device invalidated".into())),
        "stop必须保留运行时驱动故障"
    );
}
