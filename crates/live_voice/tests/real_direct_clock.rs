//! Independent wire-level diagnostic: no LiveConnection, AudioPacer or session event parser.
use base64::{engine::general_purpose::STANDARD, Engine};
use futures::{SinkExt, StreamExt};
use reqwest::header::HeaderValue;
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::{
    tungstenite::{handshake::client::generate_key, protocol::Role, Message},
    WebSocketStream,
};

/// AC18诊断：脱离产品的上传调度器和事件解析器，比较本地10秒、PCM样本数和服务端用量。
/// 数据构造：16k/24k两种官方格式，每20ms发送相应全零样本，私有Key仅进入认证头。
/// 执行过程：原始WS启动→保存resolved格式→连续10秒→close→读取原始closed用量。
/// 预期结果：关闭可确认且样本统计可复算；用量与时钟差异保留原值，不据此自动判产品通过。
#[tokio::test]
#[ignore = "explicit paid diagnostic of the service timeline"]
async fn real_live_raw_wire_clock_diagnostic() {
    let seconds = std::env::var("NORMA_LIVE_CLOCK_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10);
    assert!((1..=120).contains(&seconds), "显式时钟诊断限制为1至120秒");
    tokio::time::timeout(Duration::from_secs(seconds*2+45), async {
        let key=std::fs::read_to_string(std::env::var_os("NORMA_LIVE_TEST_KEY_FILE").expect("私有Key路径")).expect("Key可读");
        for rate in [16000,24000] {
            let mut authorization=HeaderValue::from_str(&format!("Bearer {}",key.trim())).expect("认证头有效");
            authorization.set_sensitive(true);
            let response=reqwest::Client::builder().http1_only().redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10)).build().expect("带现有代理支持的HTTP客户端")
                .get("https://api.openai.com/v1/live/sessions")
                .header("authorization",authorization).header("Connection","Upgrade").header("Upgrade","websocket")
                .header("Sec-WebSocket-Version","13").header("Sec-WebSocket-Key",generate_key())
                .send().await.unwrap_or_else(|_|panic!("官方WS握手失败；未输出认证内容"));
            assert_eq!(response.status().as_u16(),101,"原始诊断须实际升级为WS");
            let stream=response.upgrade().await.expect("升级后的原始连接");
            let mut socket=WebSocketStream::from_raw_socket(stream,Role::Client,None).await;
            socket.send(Message::Text(json!({"type":"session.start","session":{
                "model":"gpt-live-1","instructions":"静音诊断，请保持安静。","audio":{"format":{"type":"audio/pcm","rate":rate},"output":{"voice":"marin"}},"delegation":{"type":"client"},"store":false
            }}).to_string().into())).await.expect("启动写入");
            let ready=loop {
                let message=socket.next().await.expect("ready前连接不能结束").expect("ready消息完整");
                if let Message::Text(text)=message {
                    let event:Value=serde_json::from_str(&text).expect("服务端JSON");
                    if event["type"]=="session.started" {break event;}
                    assert_ne!(event["type"],"error","启动不应被拒绝");
                }
            };
            println!("raw ready: rate={rate}, id={}, resolved_audio={}",ready["session"]["id"],ready["session"]["audio"]);
            let audio=STANDARD.encode(vec![0u8;rate as usize/50*2]);
            let packet=Message::Text(json!({"type":"session.input_audio.append","audio":audio}).to_string().into());
            let origin=tokio::time::Instant::now();
            let mut clock=tokio::time::interval(Duration::from_millis(20));
            clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut frames=0;
            loop {
                tokio::select! {
                    biased;
                    _=clock.tick()=> {
                        if origin.elapsed() >= Duration::from_secs(seconds) {break;}
                        socket.send(packet.clone()).await.expect("原始PCM写入"); frames+=1;
                    }
                    event=socket.next()=> {
                        if let Some(Ok(Message::Text(text)))=event {
                            let value:Value=serde_json::from_str(&text).expect("下行JSON");
                            assert_ne!(value["type"],"error","持续PCM不应被拒绝");
                            assert_ne!(value["type"],"session.closed","不应提前关闭");
                        } else {panic!("下行流提前结束");}
                    }
                }
            }
            let active_wall=origin.elapsed().as_secs_f64();
            socket.send(Message::Text(json!({"type":"session.close"}).to_string().into())).await.expect("close写入");
            let closed=loop {
                let event=socket.next().await.expect("应等到closed").expect("关闭事件完整");
                if let Message::Text(text)=event {
                    let value:Value=serde_json::from_str(&text).expect("关闭JSON");
                    if value["type"]=="session.closed" {break value;}
                }
            };
            println!("raw closed: rate={rate}, active_wall_seconds={active_wall}, frames={frames}, pcm_seconds={}, total_after_ready_seconds={}, raw_usage={}, reason={}",
                frames as f64/50.0,origin.elapsed().as_secs_f64(),closed["usage"],closed["reason"]);
            assert!(frames>=seconds*45 && frames<=seconds*50+1,"独立发送器必须实际按50帧/秒发送，允许少量调度丢帧但不能加速");
            assert!(closed["usage"]["seconds"].is_number(),"必须保存服务端原始用量而不是本地估算");
        }
    }).await.expect("独立时钟诊断应在声明的有界期限内结束");
}
