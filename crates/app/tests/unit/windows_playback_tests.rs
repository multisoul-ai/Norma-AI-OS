use super::default_assistant_audio_sink;
use ::windows::Win32::{
    Media::Audio::{eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE},
    System::{
        Com::{CoCreateInstance, CLSCTX_ALL},
        WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    },
};
use audio::{PcmAudioChunk, PcmFormat};

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

/// 场景：Windows默认sink必须接入真实系统输出，缺硬件时也不能回退到Noop。
/// 构造：原生API独立统计输出端点；100ms、16kHz单声道PCM16静音共3200字节。
/// 执行：通过生产默认sink调度，检查启动/完成信号或精确缺设备错误。
/// 预期：有设备时真实调度完成；无设备时明确ENV，不能伪成功或返回未实现占位。
#[test]
fn windows_default_sink_uses_native_output_or_reports_absent_hardware() {
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.expect("测试线程必须初始化原生WinRT/COM");
    let _apartment = Apartment;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .expect("必须使用真实系统端点枚举器");
    let endpoints = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .expect("原生输出枚举应成功");
    let count = unsafe { endpoints.GetCount() }.expect("必须读取真实输出端点数量");
    eprintln!("Native Windows output endpoints: {count}");
    let chunk = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 3_200])
        .expect("100ms静音夹具应合法");
    let mut sink = default_assistant_audio_sink();
    let (started, start_rx) = async_channel::bounded(1);
    let (completed, completed_rx) = async_channel::bounded(1);
    let result = sink.write_chunk_with_completion(&chunk, started, completed);
    if count == 0 {
        assert_eq!(
            result,
            Err("Windows audio: no active output endpoint is available".into()),
            "缺设备应由原生后端准确拒绝，不能是Noop占位"
        );
        assert!(start_rx.try_recv().is_err(), "调度失败不能发出播放启动信号");
        assert!(
            completed_rx.try_recv().is_err(),
            "调度失败不能发出伪完成信号"
        );
        eprintln!("ENV_UNSUPPORTED: physical speaker playback; native output enumeration found no endpoints");
    } else {
        assert_eq!(
            result.expect("有原生端点时必须可调度静音PCM").as_millis(),
            100,
            "1600帧/16kHz应为100ms"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while completed_rx.is_empty()
            && !completed_rx.is_closed()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        start_rx.try_recv().expect("播放成功必须收到真实启动信号");
        completed_rx
            .try_recv()
            .expect("播放完成必须由硬件消费进度见证，不能仅返回理论时长");
    }
    sink.stop_and_confirm()
        .expect("停止必须等到原生设备确认收口");
}

/// 场景：PCM字段公开，后端不能信任绕过构造器的非法块。
/// 构造：不完整PCM16帧；执行生产默认sink写入；预期在访问设备前准确拒绝。
/// 缺物理输出不得掩盖此软件参数检查，也不得发出假完成信号。
#[test]
fn windows_default_sink_validates_pcm_before_accessing_hardware() {
    let chunk = PcmAudioChunk {
        format: PcmFormat::Pcm16,
        sample_rate_hz: 16_000,
        channels: 1,
        bytes: vec![1],
    };
    let mut sink = default_assistant_audio_sink();
    assert_eq!(
        sink.write_chunk(&chunk),
        Err("Windows audio: invalid PCM chunk: InvalidPcm16Length".into()),
        "后端必须先拒绝不完整PCM16帧"
    );
    sink.stop_and_confirm().expect("未成功启动也必须安全收口");
}

/// 场景：TTS预览必须使用同一个真实Windows后端，保留PCM参数验证和停止确认。
/// 构造：三字节PCM16非法块；执行生产preview sink；预期准确拒绝并可安全停止。
/// 无物理设备不会掩盖此软件契约，也不能回退到Noop。
#[test]
fn windows_preview_sink_preserves_native_validation_and_stop_confirmation() {
    let chunk = PcmAudioChunk {
        format: PcmFormat::Pcm16,
        sample_rate_hz: 24_000,
        channels: 1,
        bytes: vec![1, 2, 3],
    };
    let mut sink = super::default_preview_audio_sink();
    assert_eq!(
        sink.write_chunk(&chunk),
        Err("Windows audio: invalid PCM chunk: InvalidPcm16Length".into()),
        "预览必须使用原生后端的参数契约"
    );
    sink.stop_and_confirm()
        .expect("无有效调度的预览仍须安全停止");
}
