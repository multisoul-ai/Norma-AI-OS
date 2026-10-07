import assert from 'node:assert/strict';
import {once, EventEmitter} from 'node:events';
import {createServer} from 'node:http';
import {createServer as createTLSServer} from 'node:https';
import {execFileSync} from 'node:child_process';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {gunzipSync} from 'node:zlib';
import WebSocket, {WebSocketServer} from 'ws';
import {makeGateway, doubaoSession} from '../server.mjs';
import {createPlanUpstream, fetchPlan} from '../voice/plan.mjs';

const listen = async server => {server.listen(0,'127.0.0.1'); await once(server,'listening'); return server.address().port;};
const inbox = ws => {
  const events = [], waiting = [];
  ws.on('message', raw => {const event = JSON.parse(raw); waiting.length ? waiting.shift()(event) : events.push(event);});
  return async type => {
    for (;;) {
      const event = events.length ? events.shift() : await new Promise(resolve => waiting.push(resolve));
      assert.notEqual(event.type,'gateway.error',`网关不应提前失败：${JSON.stringify(event)}`);
      if (event.type === type) return event;
    }
  };
};

/// 场景：真实本机网关连接三个假套餐服务，打断仅保留已播字词，字幕晚到时隔离上行。
///
/// 数据构造：
///   上行 = 10 × 640 bytes = 200 ms PCM16/16k，验证网关到 ASR 的组包。
///   回复 = 48000 bytes = 1秒 PCM16/24k；“你好，”结束于 0.2秒，“听我说。”结束于 1秒。
///   已播放 = 200 ms，因此只保留“你好，”，绝不保留“听我说。”。
///   第一回复元数据 happy/wave 为允许动作，第二回复 angry/explode 为非法动作；两者语音文本相同。
///   静音后迟到音频 = 10 × 640 bytes = 200 ms，填充 1 区分于取消静音后的全零音频。
///   首条 ASR 上传两批 2 × 200 ms = 0.4 秒；取消静音后上传一批 = 0.2 秒。
///   两条 ASR 用量应分别在静音和挂断时送达，不能只保留前 0.4 秒。
///   本机总时限 8秒，足够两次 1秒回复、静音重建及跨过 3秒截断计时器边界。
///
/// 执行过程：
///   1. 真实 HTTP 取得票据、真实 WebSocket 连接 → 校验服务端套餐 Key 和 ASR 配置。
///   2. 发送 200 ms 麦克风 → ASR 返回部分/最终文本 → LLM/TTS 开始真实网关输出。
///   3. 收到第三个 100 ms 输出后插话，回报已播 200 ms → 字幕未返回时隔离麦克风。
///      插话前先重放角色自己的“你好，”识别回声：不得创建用户轮次，也不得停止继续播放。
///   4. 送达时间戳 → 截断确认 → 下一次 LLM 只能看到“你好，”；迟到旧音频不能出现。
///   5. 第二回复完整播放 → 静音关闭 ASR → 注入迟到音频 → 取消静音新建；迟到帧不得上传或重放。
///      第三回复按 0ms 同步截断并跨过 3秒超时。
///   6. 逻辑时钟推进 45000ms 到达静默门槛 → 主动问候作为系统指令，不虚构用户发言 → 正常挂断。
///   7. 主动问候实际结束后推进 1501ms（500ms VAD + 2×240ms 队列 + 520ms 传输余量 + 1ms），
///      用户再说与角色完全相同的一句话，必须作为新用户发言处理，不能永久按文本去重。
///   8. 正常挂断 → 在关闭确认前收到最后 0.2 秒的 ASR 用量，不因 closing 状态被丢弃。
///
/// 预期结果：
///   - 套餐配置和 PCM 正确，API Key 不进入客户端票据或事件。
///   - 第二次模型请求含听过的前缀，不含未播后缀；已确认截断不被残留计时器误关闭。
///   - 合法动作只在首段音频前发送一次，非法动作不发送，元数据不进入朗读文本。
///   - 静音后的迟到帧不触发断线、不上传或排队；取消静音后有新音频上行，正常收到关闭事件。
///   - 播放期间和播放后短时迟到的自声不进入用户历史；真正的“等等”仍立即进入插话流程。
async function checkPlanCall() {
  const fake = createServer(), peers = new WebSocketServer({server:fake}), fakePort = await listen(fake);
  const configs = [], asrAudio = [], chatBodies = [], seen = [];
  let peer, ttsController, ttsCalls = 0, time = 1000000;
  function sendASR(result) {
    const payload = Buffer.from(JSON.stringify({result})), prefix = Buffer.alloc(12);
    Buffer.from([0x11,0x91,0x10,0]).copy(prefix); prefix.writeInt32BE(1,4); prefix.writeUInt32BE(payload.length,8);
    peer.send(Buffer.concat([prefix,payload]));
  }
  peers.on('connection', ws => {
    peer = ws;
    ws.on('message', raw => {
      const payload = gunzipSync(raw.subarray(12));
      if (raw[1]>>4 === 1) {configs.push(JSON.parse(payload)); sendASR({});}
      else asrAudio.push(payload);
    });
  });
  const record = event => Buffer.from(JSON.stringify(event)+'\n');
  const pcm = Buffer.alloc(48000);
  const subtitles = {text:'你好，听我说。',words:[{word:'你好，',startTime:0,endTime:0.2},{word:'听我说。',startTime:0.2,endTime:1}]};
  const env = {COMPANION_ACCESS_TOKEN:'plan-local-test-token-at-least-24',DOUBAO_PROTOCOL:'plan',DOUBAO_SPEECH_API_KEY:'test-plan-secret'};
  const gateway = makeGateway({env,now:()=>time,upstreamFactory:(url,options) => {
    assert.equal(url,'wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async','套餐应使用已实测的专用 ASR 路由');
    assert.equal(options.headers['X-Api-Key'],env.DOUBAO_SPEECH_API_KEY,'Key 只用于网关上游鉴权');
    return new WebSocket(`ws://127.0.0.1:${fakePort}`);
  },fetchImpl:async (url,options) => {
    const body = JSON.parse(options.body);
    if (url.endsWith('/chat/completions')) {
      assert.equal(options.headers.Authorization,`Bearer ${env.DOUBAO_SPEECH_API_KEY}`,'聊天使用 Bearer 鉴权');
      assert.deepEqual(body.response_format,{type:'json_object'},'行为与回复必须由单次结构化模型输出提供');
      chatBodies.push(body);
      const invalid=chatBodies.length===2;
      return Response.json({choices:[{finish_reason:'stop',message:{content:JSON.stringify({text:'你好，听我说。',
        emotion:invalid?'angry':'happy',gesture:invalid?'explode':'wave'})}}],usage:{total_tokens:20}});
    }
    assert.equal(url,'https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional','语音合成不得调用标准实时端点');
    assert.equal(body.req_params.audio_params.format,'pcm','原生播放器需要 PCM，不应收到 MP3');
    assert.equal(body.req_params.text,'你好，听我说。','行为 JSON 不得被送入朗读');
    ttsCalls++;
    return new Response(new ReadableStream({start(controller) {
      controller.enqueue(record({code:0,data:pcm.toString('base64')}));
      if (ttsCalls === 1) ttsController = controller;
      else {controller.enqueue(record({code:0,sentence:subtitles})); controller.enqueue(record({code:20000000})); controller.close();}
    }}));
  }});
  const port = await listen(gateway);
  let client;
  const pause = ms => new Promise(resolve => setTimeout(resolve,ms));
  try {
    const ticket = await (await fetch(`http://127.0.0.1:${port}/v1/session`,{method:'POST',headers:{'Content-Type':'application/json',Authorization:`Bearer ${env.COMPANION_ACCESS_TOKEN}`},body:'{"provider":"doubao"}'})).json();
    assert.equal(JSON.stringify(ticket).includes(env.DOUBAO_SPEECH_API_KEY),false,'票据不得包含云端密钥');
    client = new WebSocket(`ws://127.0.0.1:${port}/v1/voice`,{headers:{Authorization:`Bearer ${ticket.token}`}});
    const read = inbox(client), send = event => client.send(JSON.stringify(event));
    client.on('message', raw => seen.push(JSON.parse(raw)));
    await read('session.created');
    assert.equal(configs[0].request.enable_nonstream,true,'必须启用服务端 VAD 最终断句');
    assert.equal(configs[0].request.result_type,'single','ASR 不应反复保存所有旧分句');
    for (let i=0;i<10;i++) send({type:'input_audio_buffer.append',audio:Buffer.alloc(640).toString('base64')});
    await pause(30); // Local socket dispatch only; not a service-latency measurement.
    assert.equal(asrAudio[0].length,6400,'十个原生包应合成 200 ms ASR 帧');
    const utterance = (text,end,definite) => ({utterances:[{text,start_time:end-100,end_time:end,definite}]});
    sendASR(utterance('陪我聊聊',1000,false)); await read('conversation.item.input_audio_transcription.started');
    sendASR({utterances:[...utterance('陪我聊聊',1000,false).utterances,...utterance('陪我聊聊。',1000,true).utterances]});
    const old = (await read('response.output_audio.started')).response_id;
    await read('response.output_audio.delta'); await read('response.output_audio.delta'); await read('response.output_audio.delta');
    sendASR(utterance('你好，',1200,false)); sendASR(utterance('你好，',1200,true));
    await pause(40); // 40 ms allows both local WebSocket deliveries; not an acoustic latency measurement.
    assert.equal(seen.filter(e=>e.type==='conversation.item.input_audio_transcription.started').length,1,'角色自己的外放回声不得伪造第二个用户发言开始事件');
    assert.equal(seen.filter(e=>e.type==='conversation.item.input_audio_transcription.completed').length,1,'外放回声不得完成新的用户轮次');
    assert.equal(chatBodies.length,1,'用户没有再次说话，不能因为角色自己的声音再调用模型');
    await read('response.output_audio.delta');
    const oldFramesAtInterrupt=seen.filter(e=>e.type==='response.output_audio.delta' && e.response_id===old).length;
    sendASR(utterance('等等',2000,false)); await read('conversation.item.input_audio_transcription.started');
    send({type:'gateway.interrupt',response_id:old,played_ms:200});
    for (let i=0;i<10;i++) send({type:'input_audio_buffer.append',audio:Buffer.alloc(640).toString('base64')});
    sendASR(utterance('等等，请慢一点。',2000,true));
    await pause(50);
    assert.equal(asrAudio.length,1,'字幕晚到时不得提前放行新麦克风');
    assert.equal(chatBodies.length,1,'截断确认前不得请求下一次模型回复');
    ttsController.enqueue(record({code:0,sentence:subtitles})); ttsController.enqueue(record({code:20000000})); ttsController.close();
    await read('gateway.context.truncated');
    const next = (await read('response.output_audio.started')).response_id;
    await read('response.output_audio.done');
    assert.equal(chatBodies[1].messages.find(m=>m.role==='assistant')?.content,'你好，','模型应只收到已经播放的前缀');
    assert.equal(chatBodies[1].messages.some(m=>m.role==='assistant' && m.content.includes('听我说')),false,'未播后缀不得污染下一轮上下文');
    assert.equal(asrAudio.length,2,'确认后应放行先前排队的 200 ms 音频');
    assert.equal(seen.filter(e=>e.type==='response.output_audio.delta' && e.response_id===old).length,oldFramesAtInterrupt,'打断后不得再下发旧回复音频');
    const behavior=seen.filter(e=>e.type==='gateway.behavior' && e.response_id===old);
    assert.equal(behavior.length,1,'每次回复最多在首音频前发送一次动作');
    assert.deepEqual(JSON.parse(behavior[0].arguments),{emotion:'happy',gesture:'wave'},'受限的开心与挥手应正确送达');
    assert.equal(seen.indexOf(behavior[0])+1,seen.findIndex(e=>e.type==='response.output_audio.delta' && e.response_id===old),'动作必须紧邻首段音频，不能在模型思考时提前挥手');
    assert.equal(seen.some(e=>e.type==='gateway.behavior' && e.response_id===next),false,'非法动作应被忽略，同时语音正常完成');
    send({type:'gateway.playback.done',response_id:next});
    await pause(30); // Ensure playback acknowledgement precedes the late ASR result on the other socket.
    sendASR(utterance('你好，听我说。',2500,true));
    await pause(40);
    assert.equal(chatBodies.length,2,'播放器结束后迟到的回声也不得再次请求模型');
    assert.equal(seen.filter(e=>e.type==='conversation.item.input_audio_transcription.completed').length,2,'迟到回声不得污染用户历史');
    const oldPeer = peer, closedPeer = once(oldPeer,'close');
    send({type:'input_audio_mute.commit'}); await closedPeer;
    assert.equal(oldPeer.readyState,WebSocket.CLOSED,'静音应结束识别上游');
    const lateAudio = Buffer.alloc(640,1).toString('base64');
    // 10 × 20 ms = 200 ms：模拟独立采集发送任务在静音指令后才抵达网关。
    for (let i=0;i<10;i++) send({type:'input_audio_buffer.append',audio:lateAudio});
    send({type:'input_audio_unmute.commit'});
    await pause(40);
    assert.equal(seen.some(e=>e.type==='gateway.error'),false,'静音后的合法迟到帧必须丢弃，不能误关闭通话');
    assert.equal(client.readyState,WebSocket.OPEN,'取消静音应继续使用原通话连接');
    assert.equal(configs.length,2,'取消静音应重新建立识别配置');
    assert.equal(asrAudio.length,2,'静音期间收到的音频不得在取消静音后重新上传');
    for (let i=0;i<10;i++) send({type:'input_audio_buffer.append',audio:Buffer.alloc(640).toString('base64')});
    await pause(30);
    assert.equal(asrAudio.length,3,'新 ASR 应收到取消静音后的音频');
    assert.equal(asrAudio.some(buffer=>buffer.includes(1)),false,'带 1 标记的静音迟到音频不得进入任一 ASR 连接');
    sendASR(utterance('再说一句',1000,true));
    const immediate = (await read('response.output_audio.started')).response_id;
    await read('response.output_text.done'); // All timestamps are now available: ACK runs inside sendUp synchronously.
    send({type:'gateway.interrupt',response_id:immediate,played_ms:0});
    await read('gateway.context.truncated');
    await pause(3100); // 3000 ms ACK deadline + 100 ms scheduling margin exposes an incorrectly armed leftover timer.
    assert.equal(seen.some(e=>e.type==='gateway.error'),false,'同步截断确认后不得遗留定时器而误关闭通话');
    assert.equal(JSON.stringify(seen).includes(env.DOUBAO_SPEECH_API_KEY),false,'云端密钥不得进入客户端事件');
    time+=45000; // Exactly the existing gateway's silence threshold; no real 45-second test sleep.
    send({type:'gateway.proactive'}); const greeting=(await read('response.output_audio.started')).response_id; await read('response.output_text.done');
    assert.equal(chatBodies[3].messages[0].content.includes('用户安静了一会儿'),true,'主动陪伴应使用本轮系统指令');
    assert.equal(chatBodies[3].messages.filter(m=>m.role==='user').at(-1).content,'再说一句','主动问候不能伪造新的用户发言');
    assert.equal(chatBodies[3].messages.some(m=>m.role==='user' && m.content.includes('安静了一会儿')),false,'系统主动行为不得污染用户历史');
    await read('response.output_audio.done'); send({type:'gateway.playback.done',response_id:greeting});
    await pause(30);
    time+=1501; // 1500 ms bounded echo tail + 1 ms: identical text must become eligible again.
    sendASR(utterance('你好，听我说。',2000,true)); await read('response.output_audio.started');
    assert.equal(chatBodies.length,5,'回声窗口结束后，同一句话必须重新触发用户对话');
    assert.equal(chatBodies[4].messages.filter(m=>m.role==='user').at(-1).content,'你好，听我说。','不能因为内容曾被角色说过就删除用户的新发言');
    send({type:'session.close'}); await read('session.closed');
    const asrUsage = seen.filter(e=>e.type==='gateway.usage' && e.usage?.service==='plan_asr');
    assert.equal(asrUsage.length,2,'静音与挂断都必须收到 ASR 用量，不能漏记最后一段通话');
    assert.equal(asrUsage[0].usage.uploaded_audio_seconds,0.4,'静音前实际发出的两个 200 ms 批次应计为 0.4 秒');
    assert.equal(asrUsage[1].usage.uploaded_audio_seconds,0.2,'挂断前实际发出的一个 200 ms 批次应计为 0.2 秒');
    assert.notEqual(seen.at(-1)?.type,'gateway.usage','用量必须先于 session.closed 到达，不能发到已经关闭的客户端');
  } finally {
    client?.terminate(); for (const ws of peers.clients) ws.terminate();
    gateway.closeAllConnections(); await new Promise(resolve=>gateway.close(resolve));
    peers.close(); await new Promise(resolve=>fake.close(resolve));
  }
}
/// 场景：播放尚未确认完成、数字读法和少量识别错字不能让角色自声变成用户输入。
///
/// 数据构造（含关键数值的推导过程）：
///   2400 samples × 2 bytes / 24000 Hz = 100 ms 固定假 PCM；只检验事件决策，不冒充声学降噪。
///   now 前进 5000 ms，超过旧 1500 ms 回声窗口、仍处于客户端允许的 6000 ms 播放缓冲内。
///   “2026”与“二零二六”读音相同；“蒂/缇”是一个同音识别错误，正文其余字相同。
///   反例是 2027 年和“不高兴”：分别只改一个数字或否定字，也必须保留为用户输入。
///   完成确认后 1500+1 ms 同一句话必须重新允许，不能形成永久文字黑名单。
/// 执行过程：
///   1. 通过生产 Plan 适配器发起一次回复，等实际 output_audio.done，不发送播放器完成确认。
///   2. 前进时钟并交付精确、数字读法和错字的 ASR → 检查无额外用户轮次、原回复未被打断。
///   3. 注入含否定的新用户话语 → 立即进入插话；checkPlanCall 确认播放完成后等待尾音窗口，再重复原话。
/// 预期结果：
///   - 回声不能开始用户轮次或额外调用模型；不因服务器发送结束而提前失去防护。
///   - 真正新输入及窗口外重复必须被接收，不能靠关闭麦克风通过。
async function checkEchoRecognition(genuine) {
  const text = '现在是2026年，我是米尔蒂娜，很高兴和你聊天。';
  let time = 10000;
  const events = [], requests = [], peer = new EventEmitter();
  const asr = (text, end, definite = true) => {
    const payload = Buffer.from(JSON.stringify({result:{utterances:text ? [{text,start_time:end-100,end_time:end,definite}] : []}}));
    const header = Buffer.alloc(12); Buffer.from([0x11,0x91,0x10,0]).copy(header); header.writeUInt32BE(payload.length,8);
    peer.emit('message',Buffer.concat([header,payload]),true);
  };
  peer.send = raw => {if (raw[1]>>4 === 1) queueMicrotask(()=>asr('',0));}; peer.terminate = ()=>{};
  const up = createPlanUpstream({key:'test-only',conversation:{id:'echo-check',messages:[]},now:()=>time,
    asrFactory:()=>{queueMicrotask(()=>peer.emit('open')); return peer;},fetchImpl:async(url,options)=>{
      if (url.endsWith('/chat/completions')) {
        requests.push(JSON.parse(options.body));
        return Response.json({choices:[{finish_reason:'stop',message:{content:JSON.stringify({text,emotion:'neutral',gesture:'none'})}}]});
      }
      return new Response([{code:0,data:Buffer.alloc(4800).toString('base64')},
        {code:0,sentence:{text,words:[{word:text,startTime:0,endTime:0.1}]}},{code:20000000}].map(e=>JSON.stringify(e)).join('\n'));
    }});
  const read = inbox(up); up.on('message',raw=>events.push(JSON.parse(raw)));
  up.on('error',error=>{throw error;});
  try {
    await once(up,'open'); const session = doubaoSession(); session.session.id='echo-check'; up.send(JSON.stringify(session));
    await read('session.created'); asr('请介绍一下自己',1000); await read('response.output_audio.done');
    time += 5000;
    asr(text,2000);
    assert.equal(events.filter(e=>e.type==='conversation.item.input_audio_transcription.started').length,1,'未收到实际播放完成确认时，精确回声不能打断回复');
    asr('现在是二零二六年，我是米尔蒂娜，很高兴和你聊天。',3000);
    asr('现在是2026年，我是米尔缇娜，很高兴和你聊天。',4000);
    assert.equal(events.filter(e=>e.type==='conversation.item.input_audio_transcription.completed').length,1,'数字读法和同音错字不得形成额外用户轮次');
    assert.equal(requests.length,1,'回声不得额外调用模型');
    assert.equal(events.some(e=>e.transcript?.includes('缇')),false,'回声错字不能漏进用户字幕');
    asr(genuine,5000,false);
    assert.equal(events.filter(e=>e.type==='conversation.item.input_audio_transcription.started').length,2,'真实带否定的新话语必须立即保留插话能力');
    assert.equal(events.at(-1).transcript,genuine,'不能把真实话语裁成回声或丢弃');
  } finally { up.terminate(); }
}
/// 场景：原生 HTTPS 保持流式响应和取消能力，拒绝跳转，不经线上连接不稳定的 fetch 路径。
/// 数据构造：临时本机证书只信任于本次请求；两条换行记录分开发送，首条到达后才允许发送第二条。
/// 执行过程：HTTPS POST → 先读首条 → 完成流 → 验证 307 不跟随 → 取消未完成的请求。
/// 预期结果：真实 TLS 与 POST 正文正确；首条不等 EOF，跳转目标无请求，取消后连接关闭。
async function checkNativeProviderHTTP() {
  const directory=mkdtempSync(join(tmpdir(),'norma-provider-http-'));
  const key=join(directory,'key.pem'),cert=join(directory,'cert.pem');
  execFileSync('openssl',['req','-x509','-newkey','ec','-pkeyopt','ec_paramgen_curve:P-256',
    '-nodes','-keyout',key,'-out',cert,'-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost','-days','1'],{stdio:'ignore'});
  let body='',finish,redirected=false,cancelled;
  const server=createTLSServer({key:readFileSync(key),cert:readFileSync(cert)},async(req,res)=>{
    if(req.url==='/redirect'){res.writeHead(307,{Location:'/forbidden'});res.end();return;}
    if(req.url==='/forbidden'){redirected=true;res.end();return;}
    if(req.url==='/cancel'){cancelled=once(res,'close');res.writeHead(200);res.write('pending\n');return;}
    for await(const chunk of req)body+=chunk;
    res.writeHead(200,{'Content-Type':'application/x-ndjson'});res.write('{"first":true}\n');
    finish=()=>res.end('{"last":true}\n');
  });
  const port=await listen(server),url=`https://localhost:${port}`,options={method:'POST',body:'private request',ca:readFileSync(cert)};
  try {
    const response=await fetchPlan(url,options),reader=response.body.getReader();
    const first=await reader.read();
    assert.equal(new TextDecoder().decode(first.value),'{"first":true}\n','首条数据必须在响应结束前可读');
    assert.equal(first.done,false,'不能把流式数据预先合并成完整响应');
    assert.equal(body,'private request','原生 HTTPS 必须传递原始 POST 正文');
    finish();let tail='';for(;;){const chunk=await reader.read();if(chunk.done)break;tail+=new TextDecoder().decode(chunk.value);}
    assert.equal(tail,'{"last":true}\n','后续数据不能丢失或重复');
    const redirect=await fetchPlan(url+'/redirect',options);await redirect.body.cancel();
    assert.equal(redirect.status,307,'跳转只能交由既有状态检查拒绝');
    assert.equal(redirected,false,'不能将携带供应商凭据的请求跟随到跳转目标');
    const controller=new AbortController();
    const pending=await fetchPlan(url+'/cancel',{...options,signal:controller.signal});
    controller.abort();await assert.rejects(pending.text(),undefined,'取消必须中断尚未读完的响应体');
    await cancelled;
    assert.equal(controller.signal.aborted,true,'通话取消信号必须保持取消状态');
  } finally {server.closeAllConnections();await new Promise(resolve=>server.close(resolve));rmSync(directory,{recursive:true,force:true});}
}

const deadline = setTimeout(()=>{console.error('Plan integration timed out');process.exit(1);},8000);
try {
  await checkNativeProviderHTTP();
  await checkEchoRecognition('不，我想聊一下明天的安排');
  await checkEchoRecognition('现在是2027年，我是米尔蒂娜，很高兴和你聊天。');
  await checkEchoRecognition('现在是2026年，我是米尔蒂娜，不高兴和你聊天。');
  await checkPlanCall(); console.log('PASS: Plan ASR/chat/PCM, speaker-echo rejection, genuine interruption/repetition, heard-only history, mute, close');}
finally {clearTimeout(deadline);}
