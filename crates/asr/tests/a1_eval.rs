//! G0 生产 A1 文件回放；普通测试仅验证输入、送帧和审计，不加载模型。
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use asr::{A1Config, A1RecognizerFactory, RecognizerFactory};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[path = "support/a1_eval_audit.rs"]
mod audit;
use audit::{start_manifest, PressureWatch, RunGuard};
use std::io;
use std::io::Write;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

struct MemorySample {
    peak: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MemorySample {
    fn start() -> Self {
        let listing = command("ps", &["-axo", "pid=,ppid=,args="]);
        let parent = std::process::id().to_string();
        let pid = listing
            .lines()
            .find_map(|line| {
                let mut fields = line.split_whitespace();
                let pid = fields.next()?.parse::<i32>().ok()?;
                let ppid = fields.next()?;
                (ppid == parent && line.contains("--role qwen")).then_some(pid)
            })
            .expect("只采样当前生产宿主拥有的 Qwen worker");
        let peak = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (value, done) = (peak.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let mut stats: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
                let status = unsafe {
                    libc::proc_pid_rusage(
                        pid,
                        libc::RUSAGE_INFO_V2,
                        (&mut stats as *mut libc::rusage_info_v2).cast(),
                    )
                };
                if status == 0 {
                    value.fetch_max(stats.ri_resident_size, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        Self {
            peak,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for MemorySample {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("内存采样线程应退出");
        }
    }
}

struct Shutdown;
impl Drop for Shutdown {
    fn drop(&mut self) {
        asr::shutdown_shared_a1();
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn command(program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .output()
        .expect("前置检查命令应存在");
    assert!(output.status.success(), "前置检查失败：{program}");
    String::from_utf8(output.stdout)
        .expect("命令输出应为 UTF-8")
        .trim()
        .to_owned()
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).expect("应保存可追溯信息");
}

/// 场景：结果目录已包含上一轮证据；构造原始 manifest。
/// 执行：尝试初始化新一轮；预期报 AlreadyExists 且原证据逐字节保留。
#[test]
fn existing_manifest_cannot_be_replaced_by_a_new_run() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("a0-manifest.json");
    fs::write(&path, b"original-evidence").unwrap();
    let result = start_manifest(&path, &json!({"status":"preflight"}));
    assert_eq!(
        result.unwrap_err().kind(),
        io::ErrorKind::AlreadyExists,
        "新跑测应使用独立目录"
    );
    assert_eq!(
        fs::read(path).unwrap(),
        b"original-evidence",
        "已有证据不能被部分新运行覆盖"
    );
}

/// 场景：§3.3 允许文件快速回放；T_final 从 Finish 起计，预览须区分是否实时。
/// 数据：32000 B = 16000 samples/s × 2 B × 1 s；同一音频两种回放方式。
/// 执行：以不推理的 Recognizer 边界替身接收 PCM；预期字节完全一致，
/// 实时至少 0.95 s、快速小于 0.2 s，送帧阶段不能额外调用 Finish。
#[test]
fn file_replay_can_skip_wall_clock_pacing_without_changing_pcm() {
    #[derive(Default)]
    struct Receiver {
        bytes: Vec<u8>,
        finished: bool,
    }
    impl asr::Recognizer for Receiver {
        fn accept_pcm(
            &mut self,
            chunk: &audio::PcmAudioChunk,
        ) -> Result<Vec<String>, asr::AsrError> {
            self.bytes.extend_from_slice(&chunk.bytes);
            Ok(vec!["preview".into()])
        }
        fn finish(&mut self) -> Result<Option<String>, asr::AsrError> {
            self.finished = true;
            Ok(None)
        }
    }
    let pcm = vec![0_u8; 32_000];
    let mut fast = Receiver::default();
    let start = Instant::now();
    feed_audio(&mut fast, &pcm, false).unwrap();
    assert!(
        start.elapsed() < Duration::from_millis(200),
        "快速回放不能仍等待音频墙钟"
    );
    let mut realtime = Receiver::default();
    let start = Instant::now();
    feed_audio(&mut realtime, &pcm, true).unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(950),
        "实时回放必须按音频时刻送帧"
    );
    assert_eq!(fast.bytes, pcm, "快速回放的原始 PCM 不能被修改或补齐");
    assert_eq!(realtime.bytes, pcm, "两种模式必须送达相同 PCM");
    assert!(
        !fast.finished && !realtime.finished,
        "送帧不能多调用一次终稿推理"
    );
}

fn feed_audio(
    recognizer: &mut dyn asr::Recognizer,
    pcm: &[u8],
    realtime: bool,
) -> Result<Option<f64>, asr::AsrError> {
    let start = Instant::now();
    let mut first_audio = None;
    let mut preview = None;
    for (i, bytes) in pcm.chunks(3200).enumerate() {
        if realtime {
            // 16000 samples/s × 2 bytes × 0.1 s = 3200 B。
            let deadline =
                start + Duration::from_secs_f64((i * 3200 + bytes.len()) as f64 / 32000.0);
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
        let chunk = audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16000, 1, bytes.to_vec())
            .map_err(|_| asr::AsrError::InvalidPcm)?;
        let audio_start = *first_audio.get_or_insert_with(Instant::now);
        let partials = recognizer.accept_pcm(&chunk)?;
        if preview.is_none() && partials.iter().any(|s| !s.trim().is_empty()) {
            preview = Some(audio_start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    Ok(preview)
}

/// 场景：本机真实模型按 A0 配置做生产链路回放；不接受假 worker。
/// 数据：VOICE_EVAL_CORPUS 的 PCM16 WAV 与 SHA-256；每条默认重复三次。
/// 执行：检查电源/压力/负载，prepare，按配置实时/快速送帧，Finish 计时，逐条落盘。
/// 预期：唯一 Qwen 终稿；空/错结果如实保留；中途异常不生成完整报告。
#[test]
#[ignore = "需要 Apple Silicon、本机固定 A1 模型和空闲机器"]
fn evaluate_production_a0() {
    let corpus = std::path::PathBuf::from(
        std::env::var_os("VOICE_EVAL_CORPUS").expect("设置 VOICE_EVAL_CORPUS"),
    );
    let output = std::path::PathBuf::from(
        std::env::var_os("VOICE_EVAL_OUTPUT").expect("设置 VOICE_EVAL_OUTPUT"),
    );
    fs::create_dir_all(&output).expect("创建结果目录");
    let source = fs::read(&corpus).expect("读取语料");
    let samples: Vec<Value> = if corpus.extension().is_some_and(|v| v == "json") {
        let data: Value = serde_json::from_slice(&source).unwrap();
        data["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let mut s = s.clone();
                s["source"] = data["source"].clone();
                s
            })
            .collect()
    } else {
        String::from_utf8(source.clone())
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("合法 JSONL"))
            .collect()
    };
    assert!(!samples.is_empty(), "不接受空语料");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let public = samples
        .iter()
        .all(|s| matches!(s["source"].as_str(), Some("synthetic-dev" | "regression")));
    if !public {
        assert!(
            !output.canonicalize().unwrap().starts_with(repo),
            "真人转写必须只写仓库外的本机私有目录"
        );
    }
    let config = A1Config::from_model_root(
        &std::env::var_os("VOICE_EVAL_MODEL_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| asr::default_model_installation_root().unwrap()),
    )
    .unwrap();
    let repeats: usize = std::env::var("VOICE_EVAL_REPEATS")
        .unwrap_or_else(|_| "3".into())
        .parse()
        .unwrap();
    assert!(repeats > 0, "重复次数必须大于零");
    let pacing = std::env::var("VOICE_EVAL_REALTIME").unwrap_or_else(|_| "1".into());
    assert!(
        matches!(pacing.as_str(), "0" | "1"),
        "VOICE_EVAL_REALTIME 只接受 0 或 1"
    );
    let realtime = pacing == "1";
    let mut manifest = json!({
        "commit": command("git", &["rev-parse", "HEAD"]),
        "dirty": !command("git", &["status", "--porcelain"]).is_empty(),
        "source_hashes": {
            "worker": hash(include_bytes!("../runtime/a1/worker.py")),
            "host": hash(include_bytes!("../src/providers/local/a1/host.rs")),
            "transport": hash(include_bytes!("../src/providers/local/a1/transport.rs")),
            "audit": hash(include_bytes!("support/a1_eval_audit.rs")),
            "runner": hash(include_bytes!("a1_eval.rs"))
        },
        "configuration": {"group":"A0", "hotwords":[], "system_prompt":null,
            "repeats":repeats, "realtime":realtime, "resource_poll_ms":100, "runtime":config},
        "corpus_sha256":hash(&source), "corpus_file":corpus, "corpus_samples":samples.len(),
        "machine": command("sysctl", &["-n", "hw.model"]),
        "pressure": command("sysctl", &["-n", "kern.memorystatus_vm_pressure_level"]),
        "power":command("pmset", &["-g", "batt"]),
        "load":command("uptime", &[]),
        "started_at": command("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]),
        "status":"preflight", "scope":"production A1 file replay; not microphone/App evidence"
    });
    manifest["config_sha256"] =
        hash(&serde_json::to_vec(&manifest["configuration"]).unwrap()).into();
    let mut run_guard =
        RunGuard::start(&output.join("a0-manifest.json"), &manifest).expect("禁止复用已有结果目录");
    let pressure_watch =
        PressureWatch::start(&output.join("resource-events.jsonl")).expect("应记录压力变化");
    let snapshot = if corpus.extension().is_some_and(|v| v == "json") {
        "corpus-snapshot.json"
    } else {
        "corpus-snapshot.jsonl"
    };
    fs::write(output.join(snapshot), &source).expect("保留本次语料清单快照");
    assert_eq!(
        manifest["pressure"], "1",
        "内存压力非 NORMAL，不开始模型跑测"
    );
    assert!(
        manifest["power"].as_str().unwrap().contains("AC Power"),
        "实验必须接电源"
    );
    let mut load = [0_f64; 3];
    assert_eq!(
        unsafe { libc::getloadavg(load.as_mut_ptr(), 3) },
        3,
        "应能读取机器负载"
    );
    let cpus: f64 = command("sysctl", &["-n", "hw.logicalcpu"]).parse().unwrap();
    assert!(
        load[0] <= cpus / 2.0,
        "机器不空闲：一分钟负载 {}，逻辑 CPU {}；阈值 CPU/2",
        load[0],
        cpus
    );
    let versions = command(config.python.to_str().unwrap(), &["-I", "-B", "-c",
        "import importlib.metadata,json;print(json.dumps({n:importlib.metadata.version(n) for n in ['mlx','mlx-audio','numpy','sherpa-onnx','transformers']}))"]);
    manifest["versions"] = serde_json::from_str(&versions).unwrap();
    write_json(&output.join("a0-manifest.json"), &manifest);
    let factory = A1RecognizerFactory::new(config);
    let _shutdown = Shutdown;
    let ready = Instant::now();
    factory.prepare().expect("生产模型应准备就绪");
    manifest["t_ready_ms"] = ready.elapsed().as_secs_f64().mul_add(1000.0, 0.0).into();
    manifest["status"] = "running".into();
    write_json(&output.join("a0-manifest.json"), &manifest);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("asr-A0.jsonl"))
        .expect("结果已存在时禁止覆盖，请使用新的目录");
    let mut warm = false;
    for sample in &samples {
        let audio = corpus
            .parent()
            .unwrap()
            .join(sample["audio"].as_str().expect("audio 路径"));
        let bytes = fs::read(&audio).expect("读取 WAV");
        assert_eq!(
            hash(&bytes),
            sample["sha256"].as_str().unwrap(),
            "音频完整性必须一致"
        );
        let pcm = pcm16_wav(&bytes).expect("只接受 16 kHz mono PCM16");
        for repeat in 0..repeats {
            let mut recognizer = factory.create().expect("创建生产 A1 会话");
            let memory = MemorySample::start();
            let preview =
                feed_audio(&mut *recognizer, &pcm, realtime).expect("生产 worker 接受 PCM");
            let finish = Instant::now();
            let text = recognizer
                .finish()
                .expect("唯一终稿；不允许退化到预览")
                .unwrap_or_default();
            let final_ms = finish.elapsed().as_secs_f64() * 1000.0;
            let peak = memory.peak.load(Ordering::Relaxed);
            drop(memory);
            drop(recognizer);
            let row = json!({"id":sample["id"], "repeat":repeat, "group":"A0", "text":text, "source":sample["source"],
                "audio_sha256":sample["sha256"], "warm":warm, "realtime":realtime, "t_preview_ms":preview,
                "t_ready_ms":if warm {Value::Null} else {manifest["t_ready_ms"].clone()},
                "t_final_ms":final_ms, "qwen_peak_rss_bytes":peak, "memory_scope":"Qwen resident size sampled every 20ms; not process tree or Metal peak",
                "config_sha256":manifest["config_sha256"]});
            serde_json::to_writer(&mut file, &row).unwrap();
            writeln!(file).unwrap();
            file.flush().unwrap();
            warm = true;
            eprintln!("{} repeat={} T_final={final_ms:.2}ms", sample["id"], repeat);
        }
    }
    asr::shutdown_shared_a1();
    pressure_watch.stop().expect("压力记录应完整结束");
    manifest["status"] = "complete".into();
    manifest["finished_at"] = command("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).into();
    write_json(&output.join("a0-manifest.json"), &manifest);
    run_guard.complete();
}

fn pcm16_wav(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "需要完整 16 kHz mono PCM16 WAV");
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid());
    }
    let u32_at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
    if u32_at(4) + 8 != bytes.len() {
        return Err(invalid());
    }
    let mut offset = 12;
    let mut format = false;
    let mut pcm = None;
    while offset + 8 <= bytes.len() {
        let size = u32_at(offset + 4);
        let start = offset + 8;
        let end = start.checked_add(size).ok_or_else(invalid)?;
        if end > bytes.len() {
            return Err(invalid());
        }
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                if format {
                    return Err(invalid());
                }
                format = size >= 16
                    && bytes[start..start + 4] == [1, 0, 1, 0]
                    && u32_at(start + 4) == 16_000
                    && u32_at(start + 8) == 32_000
                    && bytes[start + 12..start + 16] == [2, 0, 16, 0];
                if !format {
                    return Err(invalid());
                }
            }
            b"data" if pcm.is_none() => pcm = Some(bytes[start..end].to_vec()),
            b"data" => return Err(invalid()),
            _ => {}
        }
        offset = end + size % 2;
    }
    let pcm = pcm.ok_or_else(invalid)?;
    if !format
        || offset != bytes.len()
        || pcm.is_empty()
        || pcm.len() % 2 != 0
        || pcm.len() > 1_920_000
    {
        return Err(invalid());
    }
    Ok(pcm)
}

/// 场景：识别器只接受 16 kHz/mono/PCM16；构造含 JUNK chunk 的合法 WAV。
/// 执行：读取 PCM 后把采样率改为 48 kHz；预期正确跳过扩展 chunk、拒绝错误格式。
#[test]
fn wav_input_checks_format_and_preserves_pcm() {
    let mut bytes = b"RIFF\x32\x00\x00\x00WAVEfmt \x10\x00\x00\x00\x01\x00\x01\x00\x80\x3e\x00\x00\x00\x7d\x00\x00\x02\x00\x10\x00JUNK\x02\x00\x00\x00xxdata\x04\x00\x00\x00\x01\x00\xff\x7f".to_vec();
    assert_eq!(
        pcm16_wav(&bytes).expect("合法 WAV 应读取"),
        [1, 0, 255, 127],
        "PCM 应保持逐字节一致"
    );
    let mut duplicated = bytes.clone();
    duplicated.extend_from_slice(&bytes[12..36]);
    let riff_size = (duplicated.len() - 8) as u32;
    duplicated[4..8].copy_from_slice(&riff_size.to_le_bytes());
    assert!(
        pcm16_wav(&duplicated).is_err(),
        "重复 fmt chunk 不能悄悄覆盖输入格式"
    );
    bytes[24..28].copy_from_slice(&48_000_u32.to_le_bytes());
    assert!(pcm16_wav(&bytes).is_err(), "非 16 kHz 不能误送给识别器");
}
