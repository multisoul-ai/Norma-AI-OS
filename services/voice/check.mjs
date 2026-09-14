import assert from 'node:assert/strict';
import {once} from 'node:events';
import {createServer} from 'node:http';
import WebSocket, {WebSocketServer} from 'ws';
import {makeGateway, doubaoSession} from './server.mjs';
import {encodeDoubao2, decodeDoubao2} from './doubao2.mjs';

const listen = async server => {server.listen(0, '127.0.0.1'); await once(server, 'listening'); return server.address().port;};
const close = server => new Promise(resolve => server.close(resolve));
const inbox = (ws, parse = JSON.parse) => {
  const queued = [], waiting = [];
  ws.on('message', raw => {const value = parse(raw); waiting.length ? waiting.shift()(value) : queued.push(value);});
  return () => queued.length ? Promise.resolve(queued.shift()) : new Promise((resolve, reject) => {
    // 2 seconds permits local socket scheduling; real network latency is not being measured here.
    const timer = setTimeout(() => reject(new Error('Missing expected WebSocket event')), 2000);
    waiting.push(value => {clearTimeout(timer); resolve(value);});
  });
};

// Independent test peer uses the documented wire layout, never the production encoder.
function serverFrame(event, payload, session = 'test') {
  const audio = Buffer.isBuffer(payload), data = audio ? payload : Buffer.from(JSON.stringify(payload));
  const id = Buffer.from(session), header = Buffer.from([0x11, audio ? 0xb4 : 0x94, audio ? 0 : 0x10, 0]);
  const integer = n => {const b = Buffer.alloc(4); b.writeUInt32BE(n); return b;};
  return Buffer.concat([header, integer(event), integer(id.length), id, integer(data.length), data]);
}
function clientFrame(raw) {
  const event = raw.readUInt32BE(4), length = event >= 100 ? raw.readUInt32BE(8) : 0;
  const id = event >= 100 ? raw.subarray(12, 12 + length).toString() : '';
  const offset = event >= 100 ? 12 + length : 8;
  const body = raw.subarray(offset + 4);
  assert.equal(body.length, raw.readUInt32BE(offset), 'Client payload length must count UTF-8 bytes after session ID');
  return {event, id, payload: event === 200 ? body : JSON.parse(body)};
}

/// 场景：2.0 二进制协议与官方 StartConnection 示例一致，并拒绝损坏或错类帧。
/// 数据构造（含关键数值的推导过程）：
///   官方示例 = 4 字节头 + 4 字节事件 1 + 4 字节长度 2 + JSON {} = 14 字节。
///   音频 = 24000 × 2 × 0.020 = 960 字节；测试会话 ID 使用 4 字节的 test。
/// 执行过程：
///   1. 编码连接事件，对比官方给出的完整固定字节序列。
///   2. 独立组装服务端 352 音频帧 → 解码，逐项验证事件、会话与音频。
///   3. 增加尾字节、截去一个字节、改成客户端消息类型 → 分别拒绝。
/// 预期结果：
///   - 合法控制/PCM 帧与官方布局一致；长度不符、缺字节、错误方向均不接受。
function checkDoubao2Wire() {
  assert.deepEqual([...encodeDoubao2(1)], [17, 20, 16, 0, 0, 0, 0, 1, 0, 0, 0, 2, 123, 125], 'StartConnection must match the official byte example, not a self-roundtrip');
  const pcm = Buffer.alloc(960), wire = serverFrame(352, pcm), parsed = decodeDoubao2(wire);
  assert.equal(parsed.event, 352, 'Audio-only responses must decode as TTSResponse 352');
  assert.equal(parsed.id, 'test', 'Session ID must be decoded before the payload size');
  assert.deepEqual(parsed.payload, pcm, 'Binary PCM must not be decoded as JSON or altered');
  assert.throws(() => decodeDoubao2(Buffer.concat([wire, Buffer.from([0])])), /trailing_bytes/, 'Trailing bytes must not hide malformed framing');
  assert.throws(() => decodeDoubao2(wire.subarray(0, wire.length - 1)), /short_frame/, 'Truncated audio must not be silently played');
  const wrongDirection = Buffer.from(wire); wrongDirection[1] = 0x24;
  assert.throws(() => decodeDoubao2(wrongDirection), /unsupported_header/, 'Client audio frames must not be accepted as provider output');
  assert.throws(() => encodeDoubao2(513, {item_id: 'reply', audio_end_ms: 0}), /session_id_required/, 'Truncation must never target an unspecified provider session');
}

/// 场景：使用 2.0 的真实 reply_id 与播放时长截断；ACK 前隔离音频，失败不能恢复旧历史。
/// 数据构造（含关键数值的推导过程）：
///   下发 4800 字节 = 24000 × 2 × 0.1 秒；客户端已播放 40 ms，只能保留前 40 ms。
///   排队上行 640 字节 = 16000 × 2 × 0.020 秒；ACK 超时为 1000 ms。
///   每个新会话将模拟时钟推进 60000 ms，避免 6 次/分钟票据限制影响恢复断言。
/// 执行过程：
///   1. 二进制连接/会话握手 → 检查 2.0 模型、凭证、PCM 与截断开关。
///   2. 旧回复播放 40 ms → 提交 513 → ACK 前不上传新音频、不接受旧播放完成信号。
///   3. 收到 570 → 放行排队上行，新回复播放完成后关闭；恢复时保留 dialog-a。
///   4. 第二次截断不发 ACK → 超时关闭 → 第三次必须新建上下文。
///   5. 第三次伪造 101 ms（超过下发 100 ms）→ 拒绝，不能发送 513。
///   6. 第四次在截断 ACK 前返回新回复 → 暂停，不能让旧历史驱动的回复进入播放。
/// 预期结果：
///   - 513 的 item_id 是官方 reply_id，audio_end_ms 是 40 而非收到的 100。
///   - 旧音频不出现，合法新回复存在；不会发送仅 push_to_talk 支持的 515。
///   - 超时或越界不能携带受污染上下文重连，也不向客户端暴露服务端密钥。
async function checkDoubao2Gateway() {
  const fake = createServer(), peers = new WebSocketServer({server: fake}), fakePort = await listen(fake);
  let upstream, readUp, events, time = 1000000, sid;
  peers.on('connection', ws => {
    upstream = ws; readUp = inbox(ws, clientFrame); events = [];
    ws.on('message', raw => events.push(clientFrame(raw)));
  });
  const env = {COMPANION_ACCESS_TOKEN: 'doubao2-test-gateway-at-least-24', DOUBAO_PROTOCOL: '2',
    DOUBAO_APP_ID: 'test-app-id', DOUBAO_ACCESS_KEY: 'test-long-term-secret'};
  const gateway = makeGateway({env, now: () => time, upstreamFactory: (url, options) => {
    assert.equal(url, 'wss://openspeech.bytedance.com/api/v3/realtime/dialogue', '2.0 must not use the incompatible duplex/3.0 endpoint');
    assert.equal(options.headers['X-Api-App-ID'], env.DOUBAO_APP_ID, 'Speech App ID is required for 2.0');
    assert.equal(options.headers['X-Api-Access-Key'], env.DOUBAO_ACCESS_KEY, 'Provider credential must be sent only on the server connection');
    assert.equal(options.headers['X-Api-Key'], undefined, '3.0 API Key header is not interchangeable with 2.0 credentials');
    return new WebSocket(`ws://127.0.0.1:${fakePort}`);
  }});
  const port = await listen(gateway);
  let client, readClient;
  const send = (event, payload) => upstream.send(serverFrame(event, payload, sid));
  const control = event => client.send(JSON.stringify(event));
  const open = async () => {
    time += 60000; // Fresh one-minute ticket window for each recovery scenario.
    const res = await fetch(`http://127.0.0.1:${port}/v1/session`, {method: 'POST',
      headers: {'Content-Type': 'application/json', Authorization: `Bearer ${env.COMPANION_ACCESS_TOKEN}`},
      body: JSON.stringify({provider: 'doubao', resume: true})});
    assert.equal(res.status, 200, 'A closed previous call must release the personal gateway slot');
    const ticket = await res.json();
    assert.equal(JSON.stringify(ticket).includes(env.DOUBAO_ACCESS_KEY), false, 'Long-term Doubao key must never appear in the phone ticket');
    const connection = once(peers, 'connection');
    client = new WebSocket(`ws://127.0.0.1:${port}/v1/voice`, {headers: {Authorization: `Bearer ${ticket.token}`}});
    readClient = inbox(client); await once(client, 'open'); await connection;
    assert.equal((await readUp()).event, 1, 'StartConnection must precede StartSession');
    upstream.send(serverFrame(50, {}));
    const config = await readUp(); sid = config.id;
    assert.equal(config.event, 100, 'ConnectionStarted must trigger StartSession 100');
    send(150, {dialog_id: 'dialog-a'});
    assert.equal((await readClient()).type, 'session.created', 'Phone capture may begin only after SessionStarted');
    return config.payload;
  };
  const finish = async () => {
    const closed = once(client, 'close'), providerClosed = once(upstream, 'close');
    assert.equal((await readUp()).event, 102, 'All end/error paths must send FinishSession');
    send(152, {});
    assert.equal((await readClient()).type, 'session.closed', 'The phone must receive the provider close ACK');
    await Promise.all([closed, providerClosed]);
  };
  try {
    const config = await open();
    assert.equal(config.dialog.extra.model, '1.2.1.1', 'Truncation requires the documented O2.0 model');
    assert.equal(config.dialog.extra.enable_conversation_truncate, true, 'Audio-time truncation must be explicitly enabled');
    assert.equal(config.dialog.extra.input_mod, 'keep_alive', 'Mute must not cause an audio idle failure');
    assert.notEqual(config.dialog.extra.input_mod, 'push_to_talk', 'Natural dialogue must keep server VAD');
    assert.equal(config.tts.audio_config.format, 'pcm_s16le', 'Native PCM playback cannot consume the default OGG/Opus');
    assert.equal(config.dialog.dialog_id, undefined, 'First connection must start fresh');
    send(350, {reply_id: 'reply-a'}); await readClient();
    send(352, Buffer.alloc(4800)); await readClient();
    send(450, {question_id: 'question-b'}); await readClient();
    send(451, {results: [{text: '你好', is_interim: true}]});
    assert.equal((await readClient()).transcript, '你好', '2.0 recognition must be a replacement transcript');
    send(451, {results: [{text: '你好呀', is_interim: false}]});
    assert.equal((await readClient()).transcript, '你好呀', 'Cumulative recognition must not duplicate earlier text');
    control({type: 'gateway.interrupt', response_id: 'reply-a', played_ms: 40});
    const truncation = await readUp();
    assert.equal(truncation.event, 513, 'Full-duplex interruption must use ConversationTruncate');
    assert.equal(truncation.payload.item_id, 'reply-a', 'Official reply_id is the assistant item_id in 2.0');
    assert.equal(truncation.payload.audio_end_ms, 40, 'Only the actually played 40 ms may remain in context');
    assert.notEqual(truncation.payload.audio_end_ms, 100, 'Received 100 ms must never be treated as heard audio');
    control({type: 'input_audio_buffer.append', audio: Buffer.alloc(640).toString('base64')});
    control({type: 'gateway.playback.done', response_id: 'reply-a'});
    control({type: 'gateway.interrupt', response_id: 'reply-a', played_ms: 40});
    // Two ordered WebSocket ping barriers prove both hops drained; no scheduling sleeps.
    let pong = once(client, 'pong'); client.ping(); await pong;
    pong = once(upstream, 'pong'); upstream.ping(); await pong;
    assert.equal(events.some(e => e.event === 200), false, 'Microphone data must not reach inference before the truncation ACK');
    assert.equal(events.filter(e => e.event === 513).length, 1, 'Duplicate interrupt must not enqueue a second unmatchable ACK');
    send(352, Buffer.alloc(960)); // Old audio in flight while ACK is pending must be discarded.
    send(570, {});
    assert.equal((await readClient()).type, 'gateway.context.truncated', 'Old audio must not precede the truncation acknowledgement');
    assert.equal((await readUp()).payload.length, 640, 'ACK must release the exact queued 20 ms capture frame');
    send(350, {reply_id: 'reply-b'});
    assert.equal((await readClient()).response_id, 'reply-b', 'New reply must remain available after successful truncation');
    send(352, Buffer.alloc(960)); await readClient(); send(359, {reply_id: 'reply-b'}); await readClient();
    control({type: 'gateway.playback.done', response_id: 'reply-b'});
    control({type: 'session.close'}); await finish();
    assert.equal(events.some(e => e.event === 515), false, 'ClientInterrupt 515 is not supported in keep_alive mode');
    assert.equal((await open()).dialog.dialog_id, 'dialog-a', 'Acknowledged truncation and heard new reply should preserve resumable history');
    send(350, {reply_id: 'reply-unacked'}); await readClient(); send(352, Buffer.alloc(4800)); await readClient();
    control({type: 'gateway.interrupt', response_id: 'reply-unacked', played_ms: 0}); await readUp();
    assert.equal((await readClient()).error.code, 'context_sync_timeout', 'A missing 570 ACK must close the unsafe history');
    await finish();
    assert.equal((await open()).dialog.dialog_id, undefined, 'Unacknowledged truncation must never resume the old dialog');
    send(350, {reply_id: 'reply-invalid'}); await readClient(); send(352, Buffer.alloc(4800)); await readClient();
    control({type: 'gateway.interrupt', response_id: 'reply-invalid', played_ms: 101}); // 100 ms received + 1 ms impossible progress.
    assert.equal((await readClient()).error.code, 'invalid_client_event', 'Progress past received audio must fail validation');
    await finish();
    assert.equal(events.some(e => e.event === 513), false, 'Invalid playback progress must never reach the provider truncate endpoint');
    assert.equal((await open()).dialog.dialog_id, undefined, 'Invalid progress must invalidate resumable history');
    send(350, {reply_id: 'reply-race-old'}); await readClient(); send(352, Buffer.alloc(960)); await readClient();
    control({type: 'gateway.interrupt', response_id: 'reply-race-old', played_ms: 0}); await readUp();
    send(350, {reply_id: 'reply-race-new'});
    const race = await readClient();
    assert.equal(race.error.code, 'context_sync_race', 'Reply generation before the truncate ACK must stop the unsafe call');
    assert.notEqual(race.type, 'response.output_audio.started', 'An uncorrected-history reply must never start playing');
    await finish();
  } finally {
    client?.terminate(); upstream?.terminate(); peers.close(); gateway.closeAllConnections(); fake.closeAllConnections();
    await Promise.all([close(gateway), close(fake)]);
  }
}

/// 场景：真实本地 HTTP/WebSocket 握手验证临时票据、音频、打断及密钥隔离。
/// 数据构造：
///   输入音频 = 16000 samples/s × 2 bytes/sample × 0.020 s = 640 bytes。
///   输出音频 = 24000 samples/s × 2 bytes/sample × 0.100 s = 4800 bytes。
///   played_ms = 40 ms，小于已下发 100 ms；不能以 100 ms 当作实际已播时长。
/// 执行过程：
///   1. 未认证请求 → 401；认证请求 → 一次性 30 秒票据。
///   2. 消费票据连接 → 上游 session.create → session.created 后发送 20 ms 音频。
///   3. 下发旧回复 → 客户端打断 → 取消并结束无法准确截断的 3.0 上下文。
///   4. 上游确认关闭；再次使用旧票据 → 拒绝。
/// 预期结果：
///   - 合法 PCM 保留；打断后不得猜测 item ID，不得继续使用未听上下文。
///   - 长期 provider key 不进入客户端，错误和重放不能建立上游连接。
async function checkGateway() {
  const fake = createServer(), upstreamServer = new WebSocketServer({server: fake});
  const fakePort = await listen(fake);
  let upstream, readUp;
  upstreamServer.on('connection', ws => {upstream = ws; readUp = inbox(ws);});
  let fetchBody;
  const key = 'test-provider-key-never-in-client';
  const env = {COMPANION_ACCESS_TOKEN: 'test-gateway-token-at-least-24', DOUBAO_PROTOCOL: '3', DOUBAO_API_KEY: key,
    OPENAI_API_KEY: key, PUBLIC_ORIGIN: 'http://127.0.0.1:8787'};
  const gateway = makeGateway({env, upstreamFactory: () => new WebSocket(`ws://127.0.0.1:${fakePort}`),
    fetchImpl: async (url, options) => {
      assert.equal(url, 'https://api.openai.com/v1/realtime/client_secrets', 'OpenAI must mint on the official client_secrets endpoint');
      fetchBody = JSON.parse(options.body);
      return Response.json({value: 'ephemeral-test-token', expires_at: 2000000000});
    }});
  const port = await listen(gateway), base = `http://127.0.0.1:${port}`;
  const mint = (provider, auth = true) => fetch(`${base}/v1/session`, {method: 'POST',
    headers: {'Content-Type': 'application/json', ...(auth ? {Authorization: `Bearer ${env.COMPANION_ACCESS_TOKEN}`} : {})},
    body: JSON.stringify({provider})});
  let client;
  try {
    assert.equal((await mint('doubao', false)).status, 401, 'Unauthenticated minting would expose paid provider access');
    assert.equal((await mint('invalid')).status, 400, 'Unknown providers must not silently spend with another provider');
    const openai = await (await mint('openai')).json();
    assert.equal(openai.transport, 'webrtc', 'OpenAI should prioritize WebRTC for mobile');
    assert.equal(openai.token, 'ephemeral-test-token', 'Only short-lived OpenAI token belongs in the response');
    assert.equal(JSON.stringify(openai).includes(key), false, 'Long-term OpenAI key leaked into client response');
    assert.equal(fetchBody.session.audio.input.turn_detection.interrupt_response, true, 'User speech must cancel old OpenAI output');
    const tokenResponse = await mint('doubao'), ticket = await tokenResponse.json();
    assert.equal(tokenResponse.status, 200, 'Authorized Doubao mint should succeed');
    assert.equal(ticket.inputRate, 16000, 'Doubao capture must use the documented 16 kHz input');
    assert.equal(JSON.stringify(ticket).includes(key), false, 'Long-term Doubao key leaked into client response');
    client = new WebSocket(`ws://127.0.0.1:${port}/v1/voice`, {headers: {Authorization: `Bearer ${ticket.token}`}});
    const readClient = inbox(client);
    await once(client, 'open');
    // Observe the real upstream connection before reading its first event.
    if (!upstream) await once(upstreamServer, 'connection');
    const config = await readUp();
    assert.equal(config.session.model, '1.2.6.1', 'Duplex 3.0 requires its documented fixed model version');
    assert.equal(config.extension.tts.audio_config.format, 'pcm_s16le', 'Float32 output would be misinterpreted as int16 PCM');
    upstream.send(JSON.stringify({type: 'session.created', session: {id: 'session-test'}}));
    assert.equal((await readClient()).type, 'session.created', 'Client must wait for upstream readiness');
    assert.equal((await mint('doubao')).status, 429, 'Only one paid call may be active in this personal gateway');
    const pcm = Buffer.alloc(640).toString('base64');
    client.send(JSON.stringify({type: 'input_audio_buffer.append', audio: pcm}));
    assert.equal((await readUp()).audio, pcm, 'Capture bytes must arrive at the provider without lossy conversion');
    upstream.send(JSON.stringify({type: 'response.output_audio.started', response_id: ''}));
    assert.equal((await readClient()).type, 'response.output_audio.started', 'Official 3.0 started events may precede assignment of response_id');
    upstream.send(JSON.stringify({type: 'response.output_audio.delta', response_id: 'old', delta: Buffer.alloc(4800).toString('base64')}));
    assert.equal((await readClient()).response_id, 'old', 'Old response is valid before an interruption');
    client.send(JSON.stringify({type: 'gateway.interrupt', response_id: 'old', played_ms: 40}));
    assert.equal((await readUp()).type, 'response.cancel', 'Interruption must cancel provider generation');
    const end = await readUp();
    assert.equal(end.type, 'session.close', 'Undocumented 3.0 truncation must end the contaminated session');
    assert.notEqual(end.type, 'conversation.item.update', 'response_id must never be guessed to be an assistant item ID');
    assert.equal((await readClient()).error.code, 'context_truncation_unavailable', 'The client must see an explicit recoverable UI state instead of silent context corruption');
    upstream.send(JSON.stringify({type: 'response.output_audio.delta', response_id: 'old', delta: pcm}));
    upstream.send(JSON.stringify({type: 'response.output_audio.delta', response_id: 'new', delta: pcm}));
    upstream.send(JSON.stringify({type: 'session.closed'}));
    const after = await readClient();
    assert.equal(after.type, 'session.closed', 'No new reply may run against the uncorrected history');
    assert.notEqual(after.response_id, 'old', 'Delayed old reply audio must be discarded after interruption');
    client.terminate();
    const replay = new WebSocket(`ws://127.0.0.1:${port}/v1/voice`, {headers: {Authorization: `Bearer ${ticket.token}`}});
    const [error] = await once(replay, 'error');
    assert.match(error.message, /401/, 'Spent WebSocket tickets must never be accepted again');
    assert.equal(doubaoSession('voice-test').session.audio.output.voice, 'voice-test', 'Voice must stay configurable for account entitlement');
  } finally {
    client?.terminate(); upstream?.terminate(); upstreamServer.close();
    gateway.closeAllConnections(); fake.closeAllConnections();
    await Promise.all([close(gateway), close(fake)]);
  }
}

/// 场景：关闭确认、静默冷却及断网后的上下文恢复必须尊重实际播放状态。
/// 数据构造（含关键数值的推导过程）：
///   静默阈值 = 45 秒 = 45000 ms，先推进 44999 ms（不允许），再推进 1 ms（允许）。
///   冷却阈值 = 120 秒 = 120000 ms，在首个问候后立即请求第二次（不允许）。
///   第一回复完整播放；第二回复仅收到未播放，因此第三次连接不能恢复第二次的未听历史。
/// 执行过程：
///   1. 建立 session-a → 静默边界前后分别请求主动问候，使用 mute 事件作有序屏障。
///   2. 第一回复播放完毕 → 关闭会话 → 必须收到 session.closed 才结束。
///   3. resume 连接应带 session-a；第二回复未播放就断线 → 下次必须新建上下文。
/// 预期结果：
///   - 完整播放的会话可以接续；未送达的回复不能假装已听。
///   - 静默未达标/冷却中/静音时不能主动开口；服务端必须向客户端转发关闭确认。
async function checkRecovery() {
  const fake = createServer(), upstreamServer = new WebSocketServer({server: fake});
  const fakePort = await listen(fake);
  let upstream, readUp, time = 1000000;
  upstreamServer.on('connection', ws => {
    upstream = ws; readUp = inbox(ws);
    ws.on('message', raw => {
      if (JSON.parse(raw).type === 'session.close') ws.send(JSON.stringify({type: 'session.closed'}));
    });
  });
  const passphrase = 'recovery-check-token-at-least-24';
  const gateway = makeGateway({env: {COMPANION_ACCESS_TOKEN: passphrase, DOUBAO_PROTOCOL: '3', DOUBAO_API_KEY: 'fake-key'},
    now: () => time, upstreamFactory: () => new WebSocket(`ws://127.0.0.1:${fakePort}`)});
  const port = await listen(gateway);
  let client, readClient;
  const open = async resume => {
    const request = await fetch(`http://127.0.0.1:${port}/v1/session`, {method: 'POST',
      headers: {'Content-Type': 'application/json', Authorization: `Bearer ${passphrase}`},
      body: JSON.stringify({provider: 'doubao', resume})});
    assert.equal(request.status, 200, 'Previous socket must release the single-call slot before reconnection');
    const ticket = await request.json();
    const connection = once(upstreamServer, 'connection');
    client = new WebSocket(`ws://127.0.0.1:${port}/v1/voice`, {headers: {Authorization: `Bearer ${ticket.token}`}});
    readClient = inbox(client);
    await once(client, 'open'); await connection;
    const config = await readUp();
    upstream.send(JSON.stringify({type: 'session.created', session: {id: 'session-a'}}));
    await readClient();
    return config;
  };
  const control = type => client.send(JSON.stringify({type}));
  try {
    assert.equal((await open(false)).session.id, undefined, 'Fresh call must not inherit earlier conversations');
    time += 44999; // 45000 ms - 1 ms: just below the minimum silence window.
    control('gateway.proactive'); control('input_audio_mute.commit');
    assert.equal((await readUp()).type, 'input_audio_mute.commit', '44999 ms silence must not produce a greeting');
    control('input_audio_unmute.commit'); await readUp();
    time += 1; // 44999 + 1 = 45000 ms: threshold reached.
    control('gateway.proactive');
    assert.equal((await readUp()).type, 'speech_text_buffer.commit', 'Greeting should be allowed exactly at 45000 ms');
    control('gateway.proactive'); control('input_audio_mute.commit');
    assert.equal((await readUp()).type, 'input_audio_mute.commit', 'Immediate second greeting must be suppressed during the 120000 ms cooldown');
    time += 120000; // Cooldown elapsed, but mute still overrides autonomous speech.
    control('gateway.proactive'); control('input_audio_unmute.commit');
    assert.equal((await readUp()).type, 'input_audio_unmute.commit', 'Muted user must not receive autonomous speech even after cooldown');
    upstream.send(JSON.stringify({type: 'response.output_audio.delta', response_id: 'heard', delta: Buffer.alloc(960).toString('base64')}));
    await readClient(); // 24000 × 2 × .020 = 960 bytes, one complete 20 ms playback chunk.
    client.send(JSON.stringify({type: 'gateway.playback.done', response_id: 'heard'}));
    const closed = once(client, 'close'), providerClosed = once(upstream, 'close');
    control('session.close');
    assert.equal((await readClient()).type, 'session.closed', 'Client must observe upstream close acknowledgement');
    await Promise.all([closed, providerClosed]);
    assert.equal((await open(true)).session.id, 'session-a', 'A fully heard conversation should resume with its server-owned session ID');
    upstream.send(JSON.stringify({type: 'response.output_audio.delta', response_id: 'unheard', delta: Buffer.alloc(960).toString('base64')}));
    await readClient();
    const upstreamClosed = once(upstream, 'close'); client.terminate(); await upstreamClosed;
    const fresh = await open(true);
    assert.equal(fresh.session.id, undefined, 'Abrupt disconnect with unplayed audio must not resume unheard context');
    assert.notEqual(fresh.session.id, 'session-a', 'Fresh context must not claim the old provider session');
    client.send(JSON.stringify({type: 'conversation.item.create', items: [{call_id: 'invented', role: 'tool', content: [{text: 'ok'}]}]}));
    assert.equal((await readClient()).error.code, 'invalid_client_event', 'Unknown tool call IDs must fail closed');
  } finally {
    client?.terminate(); upstream?.terminate(); upstreamServer.close();
    gateway.closeAllConnections(); fake.closeAllConnections();
    await Promise.all([close(gateway), close(fake)]);
  }
}

/// 场景：重放 Vercel 的音频到达抖动，合法积压不掉线，无时间流逝的洪泛仍被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   Vercel线上50帧/s的合法流因网络积压触发旧1.2秒突发上限；模拟30帧后659ms到达91帧。
///   每帧16000Hz × 2bytes × .020s = 640bytes；121帧=77440bytes。
///   旧60帧容量+659ms补32.95帧不足121帧；新150帧=3秒=96000bytes，低于128KiB上游背压限制。
///   新会话同刻151帧=96640bytes，必须只放行150帧；补充速率仍50帧/s，不能无限突发。
/// 执行过程：真实HTTP/WS回环 → 30+91帧抖动 → 正常挂断 → 新会话151帧同刻突发。
/// 预期结果：合法网络积压完整送达无gateway.error；突发第151帧被拒绝，持续限流仍有效。
///   超过3秒积压时结束当前通话并允许既有重连流程恢复，不应误报为不可重试的配置错误。
async function checkAudioArrivalJitter() {
  const fake=createServer(), peers=new WebSocketServer({server:fake}), port=await listen(fake);
  let delivered=0, time=1000000;
  peers.on('connection',ws=>ws.on('message',raw=>{
    const e=JSON.parse(raw);
    if(e.type==='session.create')ws.send(JSON.stringify({type:'session.created',session:{id:'jitter-session'}}));
    if(e.type==='input_audio_buffer.append')delivered++;
    if(e.type==='session.close'){ws.send('{"type":"session.closed"}');ws.close();}
  }));
  const env={COMPANION_ACCESS_TOKEN:'jitter-check-passphrase-at-least-24',DOUBAO_PROTOCOL:'3',DOUBAO_API_KEY:'fake-key'};
  const gateway=makeGateway({env,now:()=>time,upstreamFactory:()=>new WebSocket(`ws://127.0.0.1:${port}`)});
  const local=await listen(gateway),clients=[];
  const frame=JSON.stringify({type:'input_audio_buffer.append',audio:Buffer.alloc(640).toString('base64')});
  const open=async()=>{
    const r=await fetch(`http://127.0.0.1:${local}/v1/session`,{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${env.COMPANION_ACCESS_TOKEN}`},body:'{"provider":"doubao"}'});
    const ticket=await r.json(),ws=new WebSocket(`ws://127.0.0.1:${local}/v1/voice`,{headers:{Authorization:`Bearer ${ticket.token}`}});
    clients.push(ws);const read=inbox(ws);await read();return {ws,read};
  };
  // 100 × 2ms = 200ms local delivery allowance; injected time remains deterministic.
  const wait=async predicate=>{for(let i=0;i<100&&!predicate();i++)await new Promise(r=>setTimeout(r,2));};
  try {
    const first=await open();let error;
    first.ws.on('message',raw=>{const e=JSON.parse(raw);if(e.type==='gateway.error')error=e.error.code;});
    for(let i=0;i<30;i++)first.ws.send(frame);
    await wait(()=>delivered===30||error);
    assert.equal(delivered,30,'第一批 30 帧应完整到达');
    time+=659;
    for(let i=0;i<91;i++)first.ws.send(frame);
    await wait(()=>delivered===121||error);
    assert.equal(error,undefined,'实测 659ms 内的合法网络积压不得被误判为音频洪泛');
    assert.equal(delivered,121,'两批合计121帧必须完整到达，不能丢弃合法录音');
    const ended=once(first.ws,'close');first.ws.send('{"type":"session.close"}');await ended;
    const second=await open();delivered=0;
    for(let i=0;i<151;i++)second.ws.send(frame);
    const rejected=await second.read();
    assert.equal(rejected.error?.code,'audio_rate_limit','没有补充时间的第151帧必须触发明确的音频限流');
    assert.equal(rejected.error?.retryable,true,'真实网络积压应允许客户端重连，不能误报永久配置错误');
    await wait(()=>delivered===150);
    assert.equal(delivered,150,'突发上限外的第151帧不得上传到付费上游');
    assert.notEqual(delivered,151,'不能用完全取消音频限流来修复网络抖动');
  } finally {
    for(const ws of clients)ws.terminate();for(const ws of peers.clients)ws.terminate();peers.close();
    gateway.closeAllConnections();fake.closeAllConnections();await Promise.all([close(gateway),close(fake)]);
  }
}

/// 场景：供应商已关闭，Redis 历史保存稍慢时仍必须把挂断确认送达手机。
/// 数据构造：释放延迟 = 1000ms 上游关闭预算 + 200ms 调度余量；不访问真实 Redis 或付费服务。
/// 执行过程：真实 HTTP/WS 接通 → 上游立即确认挂断 → 等待模拟云存储释放 → 手机收到确认后关闭。
/// 预期结果：释放仅一次且先于手机关闭，session.closed 存在，close_timeout 和 gateway.error 不存在。
async function checkCloudCloseAcknowledgement() {
  const fake=createServer(), peers=new WebSocketServer({server:fake}), port=await listen(fake);
  peers.on('connection',ws=>ws.on('message',raw=>{
    const event=JSON.parse(raw);
    if(event.type==='session.create')ws.send('{"type":"session.created","session":{"id":"close-check"}}');
    if(event.type==='session.close'){ws.send('{"type":"session.closed"}');ws.close();}
  }));
  let releases=0, released=false;
  const sessionStore={mint:async()=>true,claim:async()=>({id:'lease',session:{id:'',expires:0}}),renew:async()=>true,
    release:async()=>{releases++;await new Promise(resolve=>setTimeout(resolve,1200));released=true;}};
  const env={COMPANION_ACCESS_TOKEN:'close-check-passphrase-at-least-24',DOUBAO_PROTOCOL:'3',DOUBAO_API_KEY:'fake-key'};
  const gateway=makeGateway({env,sessionStore,upstreamFactory:()=>new WebSocket(`ws://127.0.0.1:${port}`)});
  const local=await listen(gateway);let client;
  try {
    const response=await fetch(`http://127.0.0.1:${local}/v1/session`,{method:'POST',headers:{
      'Content-Type':'application/json',Authorization:`Bearer ${env.COMPANION_ACCESS_TOKEN}`},body:'{"provider":"doubao"}'});
    const ticket=await response.json();
    client=new WebSocket(`ws://127.0.0.1:${local}/v1/voice`,{headers:{Authorization:`Bearer ${ticket.token}`}});
    const read=inbox(client),events=[];client.on('message',raw=>events.push(JSON.parse(raw)));
    await read();const ended=once(client,'close');client.send('{"type":"session.close"}');
    const [code,reason]=await ended;
    assert.equal(released,true,'手机连接关闭前必须完成云端租约释放');
    assert.equal(releases,1,'关闭流程只能释放一次，不能重写会话历史');
    assert.equal(events.some(event=>event.type==='session.closed'),true,'已收到的上游挂断确认不能被过期计时器吞掉');
    assert.equal(events.some(event=>event.type==='gateway.error'),false,'正常云存储耗时不能产生通话错误');
    assert.equal(code,1000,'正常挂断应使用1000关闭码');
    assert.notEqual(reason.toString(),'close_timeout','上游已结束后不能再按上游超时强制关闭');
  } finally {
    client?.terminate();for(const ws of peers.clients)ws.terminate();peers.close();
    gateway.closeAllConnections();fake.closeAllConnections();await Promise.all([close(gateway),close(fake)]);
  }
}

// Includes a 1-second ACK timeout and 1.2-second cloud release; 10 seconds bounds the local checks.
const deadline = setTimeout(() => { console.error('FAIL: local integration exceeded 10 seconds'); process.exit(1); }, 10000);
deadline.unref();
try { checkDoubao2Wire(); await checkGateway(); await checkRecovery(); await checkDoubao2Gateway(); await checkAudioArrivalJitter(); await checkCloudCloseAcknowledgement(); } finally { clearTimeout(deadline); }
console.log('PASS: auth, tickets, PCM, 2.0 wire/truncate ACK/timeout, 3.0 safe stop, stale audio, tools, silence/cooldown, close ACK, safe resume');
