import { createServer } from 'node:http';
import { randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import WebSocket, { WebSocketServer } from 'ws';
import {encodeDoubao2, decodeDoubao2, doubao2Session, doubao2Event} from './doubao2.mjs';
import {createPlanUpstream, fetchPlan} from './plan.mjs';
import {verifyVoiceAuthorization} from './voice-authorization.mjs';
import {createMobileTextService} from './mobile-text.mjs';
import {createCollaborationRelay} from './collaboration-relay.mjs';

const instructions = '你是 Norma，一位温柔、自然、有自己好奇心的 AI 伙伴。你清楚自己是 AI。先给结论；简单问题一句话，必要时再补一句依据或下一步，用户要细节再展开。用适合语音的中文；不加开场寒暄、不复述、不报内部过程。认真倾听，不替用户做决定，不鼓励排他依赖。不声称能看到摄像头。用户说话时优先倾听。需要表达情绪或动作时可调用 set_behavior，避免每句话都调用。';
export const behaviorTool = {
  type: 'function', name: 'set_behavior', description: '温和地表达表情或手势；动作由客户端决定是否执行。',
  parameters: { type: 'object', additionalProperties: false, properties: {
    emotion: {type: 'string', enum: ['neutral', 'happy', 'curious', 'concerned']},
    gesture: {type: 'string', enum: ['none', 'nod', 'wave']},
  }, required: ['emotion', 'gesture'] },
};

export function doubaoSession(voice) {
  return { type: 'session.create', session: { model: '1.2.6.1', instructions,
    audio: { input: {format: {type: 'pcm', rate: 16000}},
      output: {format: {type: 'pcm', rate: 24000}, voice}}, tools: [behaviorTool] },
    extension: {tts: {audio_config: {format: 'pcm_s16le', sample_rate: 24000, channel: 1}}} };
}

function sameSecret(actual, expected) {
  const a = Buffer.from(actual || ''), b = Buffer.from(expected || '');
  return b.length >= 24 && a.length === b.length && timingSafeEqual(a, b);
}

function json(res, status, data) {
  res.writeHead(status, {'Content-Type': 'application/json', 'Cache-Control': 'no-store'});
  res.end(JSON.stringify(data));
}

async function readJSON(req) {
  let size = 0; const chunks = [];
  for await (const chunk of req) {
    size += chunk.length;
    if (size > 4096) throw new Error('request_too_large');
    chunks.push(chunk);
  }
  const value = JSON.parse(Buffer.concat(chunks).toString() || '{}');
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new SyntaxError('object_required');
  return value;
}

// Limits bound one personal deployment; add account identities when supporting multiple people.
export function makeGateway({env = process.env, fetchImpl = fetch, upstreamFactory, now = Date.now, sessionStore} = {}) {
  const passphrase = env.COMPANION_ACCESS_TOKEN;
  if (!passphrase || passphrase.length < 24) throw new Error('Set COMPANION_ACCESS_TOKEN (at least 24 characters)');
  const protocol = env.DOUBAO_PROTOCOL || '2';
  if (!['2', '3', 'plan'].includes(protocol)) throw new Error('DOUBAO_PROTOCOL must be 2, 3 or plan');
  const legacy = protocol === '2';
  const plan = protocol === 'plan', planConversation = {id:'',messages:[]};
  const origin = new URL(env.PUBLIC_ORIGIN || 'http://127.0.0.1:8787');
  if (origin.username || origin.password || !/^(\/[A-Za-z0-9_-]+)*\/?$/.test(origin.pathname) || origin.search || origin.hash ||
      (origin.protocol !== 'https:' && !(origin.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(origin.hostname)))) {
    throw new Error('PUBLIC_ORIGIN must be HTTPS, or HTTP loopback for development');
  }
  // Nginx strips the mount path upstream; issued public URLs must retain it.
  origin.pathname = origin.pathname.replace(/\/$/, '') + '/';
  const tickets = new Map();
  const toolTickets = new Map();
  let lastOwner = '';
  let lastSession = {id: '', expires: 0};
  let minted = [], active = 0, pending = 0;
  const wss = new WebSocketServer({noServer: true, maxPayload: 16384, perMessageDeflate: false});
  const taskWss = new WebSocketServer({noServer:true,maxPayload:512*1024,perMessageDeflate:false});
  const mobileText = createMobileTextService({env, fetchImpl, now});
  const collaborationRelay = createCollaborationRelay({env, authorize: req => mobileText.authorize(req), now});
  const authorized = req => {
    const token=req.headers.authorization?.replace(/^Bearer /, '');
    if (sameSecret(token,passphrase)) return 'admin';
    if (!verifyVoiceAuthorization(token,env,now())) return null;
    const claims=JSON.parse(Buffer.from(token.split('.')[1],'hex'));
    return claims.iss+':'+claims.sub;
  };
  const prune = () => {
    const currentTime = now();
    for (const [key, value] of tickets) if (value.expires <= currentTime) tickets.delete(key);
    for (const [key, value] of toolTickets) if (value.expires <= currentTime) toolTickets.delete(key);
    minted = minted.filter(t => t > currentTime - 60000);
  };
  const server = createServer(async (req, res) => {
    const path = (req.url || '').split('?')[0];
    if (path === '/health' && req.method === 'GET') return json(res, 200, {ok: true});
    if (await mobileText.handle(req, res, path)) return;
    if (path !== '/v1/session' || req.method !== 'POST') return json(res, 404, {error: 'not_found'});
    const owner=authorized(req);
    if (!owner) return json(res, 401, {error: 'unauthorized'});
    if (req.headers['content-type']?.split(';')[0] !== 'application/json') return json(res, 415, {error: 'json_required'});
    prune();
    if (minted.length >= 6 || pending > 0 || active > 0) return json(res, 429, {error: 'session_limit'});
    pending++;
    try {
      const body = await readJSON(req);
      if (!['doubao', 'openai'].includes(body.provider)) return json(res, 400, {error: 'invalid_provider'});
      const key = body.provider === 'doubao' ? (plan ? env.DOUBAO_SPEECH_API_KEY : legacy ? env.DOUBAO_ACCESS_KEY : env.DOUBAO_API_KEY) : env.OPENAI_API_KEY;
      if (!key) return json(res, 503, {error: 'provider_not_configured'});
      if (body.provider === 'doubao' && legacy && !env.DOUBAO_APP_ID) return json(res, 503, {error: 'provider_not_configured'});
      minted.push(now());
      if (body.provider === 'openai') {
        const response = await fetchImpl('https://api.openai.com/v1/realtime/client_secrets', {
          method: 'POST', signal: AbortSignal.timeout(10000),
          headers: {'Authorization': `Bearer ${key}`, 'Content-Type': 'application/json'},
          body: JSON.stringify({expires_after: {anchor: 'created_at', seconds: 60}, session: {
            type: 'realtime', model: env.OPENAI_REALTIME_MODEL || 'gpt-realtime-2.1', instructions,
            audio: {input: {turn_detection: {type: 'server_vad', silence_duration_ms: 500,
              create_response: true, interrupt_response: true}, transcription: {model: 'gpt-4o-mini-transcribe'}},
              output: {voice: 'marin'}}, tools: [behaviorTool],
          }}),
        });
        if (!response.ok) return json(res, response.status === 429 ? 429 : 502, {error: 'provider_session_failed'});
        const data = await response.json();
        if (typeof data.value !== 'string' || !Number.isFinite(data.expires_at)) throw new Error('bad_provider_response');
        return json(res, 200, {provider: 'openai', transport: 'webrtc', token: data.value, expiresAt: data.expires_at,
          url: 'https://api.openai.com/v1/realtime/calls', inputRate: 24000, outputRate: 24000});
      }
      // 32 random bytes = 256 bits; one use, 30 seconds to open the WebSocket.
      const token = randomBytes(32).toString('base64url');
      if (sessionStore) {
        if (!await sessionStore.mint(token,body.resume === true,owner)) return json(res,429,{error:'session_limit'});
      } else tickets.set(token, {owner,expires: now() + 30000, resume: body.resume === true && owner===lastOwner && lastSession.expires > now() ? lastSession.id : ''});
      const url = new URL('v1/voice', origin); url.protocol = origin.protocol === 'https:' ? 'wss:' : 'ws:';
      json(res, 200, {provider: 'doubao', transport: 'websocket', token, expiresAt: Math.floor(now()/1000)+30,
        url: url.href, inputRate: 16000, outputRate: 24000});
    } catch (error) {
      json(res, error instanceof SyntaxError || error.message === 'request_too_large' ? 400 : 502,
        {error: 'session_failed'});
    } finally { pending--; }
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 10000;
  server.on('upgrade', async (req, socket, head) => {
    if (collaborationRelay.upgrade(req, socket, head)) return;
    prune();
    const token = req.headers.authorization?.replace(/^Bearer /, '');
    if (req.url?.startsWith('/v1/tools?')) {
      if (sessionStore?.tools) {
        try {
          const media=req.url.slice('/v1/tools?call_id='.length);
          const peer=req.url==='/v1/tools?call_id='+media ? await sessionStore.tools.claim(token,media) : null;
          if (peer && !socket.destroyed) {taskWss.handleUpgrade(req,socket,head,ws=>peer.bind(ws));return;}
          peer?.close();
        } catch {socket.end('HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n');return;}
        socket.end('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n');return;
      }
      const ticket=toolTickets.get(token);
      if (!ticket || req.url !== '/v1/tools?call_id='+ticket.id) {
        socket.end('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); return;
      }
      toolTickets.delete(token);
      taskWss.handleUpgrade(req,socket,head,ticket.attach); return;
    }
    if (req.url !== '/v1/voice' || active > 0 || (!sessionStore && !tickets.has(token))) {
      socket.end('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); return;
    }
    let lease;
    if (sessionStore) {
      try { lease = await sessionStore.claim(token); }
      catch { socket.end('HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n'); return; }
      if (!lease) {socket.end('HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n'); return;}
      if (socket.destroyed) {await sessionStore.release(lease.id,null).catch(()=>{}); return;}
      lastSession = lease.session;
    }
    const previous = lease ? lease.session.id : tickets.get(token).resume;
    if (!lease) {
      const owner=tickets.get(token).owner;
      if (lastOwner!==owner) {lastSession={id:'',expires:0};planConversation.id='';planConversation.messages=[];}
      lastOwner=owner;
    }
    // A ticket minted before an interruption must not resurrect history invalidated afterwards.
    const resume = previous === lastSession.id && lastSession.expires > now() ? previous : '';
    tickets.delete(token);
    wss.handleUpgrade(req, socket, head, ws => {
      active++;
      ws.once('close', () => {active--;});
      bridge(ws, resume, lease);
    });
  });

  function bridge(client, resume, lease) {
    const conversation = lease?.conversation || planConversation;
    const upstreamURL = legacy ? 'wss://openspeech.bytedance.com/api/v3/realtime/dialogue' : 'wss://openspeech.bytedance.com/api/v3/duplex/realtime/dialogue';
    const upstreamOptions = {
      headers: legacy ? {'X-Api-App-ID': env.DOUBAO_APP_ID, 'X-Api-Access-Key': env.DOUBAO_ACCESS_KEY,
        'X-Api-Resource-Id': 'volc.speech.dialog', 'X-Api-App-Key': 'PlgvMymc7f3tQnJ6', 'X-Api-Connect-Id': randomUUID()} : {'X-Api-Key': env.DOUBAO_API_KEY},
      handshakeTimeout: 10000,
      maxPayload: 2 * 1024 * 1024, perMessageDeflate: false,
    };
    const connect = upstreamFactory || ((url, options) => new WebSocket(url, options));
    let upstream;
    try { upstream = plan ? createPlanUpstream({key:env.DOUBAO_SPEECH_API_KEY,fetchImpl,asrFactory:upstreamFactory,conversation,now})
      : connect(upstreamURL, upstreamOptions); } catch {
        if (lease) void sessionStore.release(lease.id,null).catch(()=>{});
        client.close(1011, 'upstream_unavailable'); return;
      }
    let ready = false, closing = false, muted = false, activeResponse = '', outputBytes = 0;
    let userSpeaking = false, lastUserActivity = now(), lastProactive = 0, outputActive = false;
    const wireSession = randomUUID(), legacyState = {};
    const mediaID='doubao_'+wireSession;
    let mac=null, toolToken='', macReady=false;
    let truncation = null, truncateTimer, queuedAudio = [], connectionStarted = false;
    let renewal, rotation, releasing, closingEvent, closeTimer, rotating = false, renewing = false;
    const release = () => {
      clearInterval(renewal); clearInterval(rotation);
      if (!lease) return Promise.resolve();
      return releasing ||= Promise.resolve(cloudTools?.close()).then(()=>sessionStore.release(lease.id,
        !outputActive && !truncation ? {session:lastSession,conversation} : null)).catch(()=>{});
    };
    let audioWindow = now(), audioBudget = 96000, audioUpdated = now(), controls = 0;
    const blocked = new Set(), pendingTools = new Set();
    const sendClient = data => {
      if (client.readyState !== WebSocket.OPEN) return;
      if (client.bufferedAmount > 512 * 1024) return stop('slow_client');
      client.send(JSON.stringify(data));
    };
    const sendUp = data => {
      if (upstream.readyState !== WebSocket.OPEN) return;
      if (upstream.bufferedAmount > 128 * 1024) return stop('slow_upstream');
      if (!legacy) { upstream.send(JSON.stringify(data)); return; }
      let event, payload = {};
      switch (data.type) {
        case 'session.create': event = 1; break;
        case 'session.start':
          event = 100; payload = doubao2Session(env.DOUBAO_VOICE || 'zh_female_vv_jupiter_bigtts',
            instructions.replace('需要表达情绪或动作时可调用 set_behavior，避免每句话都调用。', ''), resume); break;
        case 'session.close': event = 102; break;
        case 'input_audio_buffer.append': event = 200; payload = Buffer.from(data.audio, 'base64'); break;
        case 'input_audio_mute.commit': case 'input_audio_unmute.commit': return; // keep_alive permits no microphone input.
        case 'conversation.item.truncate': event = 513; payload = {item_id: data.id, audio_end_ms: data.played_ms}; break;
        case 'speech_text_buffer.commit': event = 300; payload = {content: data.text}; break;
        default: throw new Error('unsupported_doubao2_event');
      }
      upstream.send(encodeDoubao2(event, payload, wireSession));
    };
    const sendMac = event => {
      if (mac?.readyState !== WebSocket.OPEN) return;
      if (mac.bufferedAmount > 512*1024) {mac.terminate(); return;}
      mac.send(JSON.stringify(event));
    };
    const closeTools = () => {
      toolTickets.delete(toolToken); toolToken='';
      const old=mac; mac=null; macReady=false;
      if (plan) upstream.detachTools();
      old?.close(1000,'media_closed');
    };
    const attachTools = socket => {
      if (closing || !ready) {socket.close(1008,'media_closed'); return;}
      closeTools(); mac=socket;
      sendMac({type:'session.created',session:{id:mediaID,type:'realtime',instructions,tools:[behaviorTool]}});
      const configured=setTimeout(()=>{if (!macReady && mac===socket) socket.terminate();},5000);
      socket.on('message',(raw,binary)=>{
        if (mac!==socket || closing) return;
        try {
          if (binary) throw Error('json_required');
          const event=JSON.parse(raw);
          if (event.type==='session.update') {
            const session=event.session;
            if (macReady || session?.type!=='realtime' || typeof session.instructions!=='string' || session.instructions.length>32768 ||
              !Array.isArray(session.tools) || session.tools.length>32 ||
              session.tools.some(tool=>tool.type!=='function' || !/^[a-z_]{1,64}$/.test(tool.name) ||
                typeof tool.parameters!=='object' || !tool.parameters) ||
              new Set(session.tools.map(tool=>tool.name)).size!==session.tools.length) throw Error('invalid_tools');
            upstream.configureTools(session); macReady=true; clearTimeout(configured);
            sendMac({type:'session.updated',session:{id:mediaID,type:'realtime',tools:session.tools}});
          } else if (macReady && event.type==='conversation.item.create') upstream.toolResult(event.item);
          else if (macReady && event.type==='response.create') upstream.continueTools();
          else throw Error('invalid_tool_event');
        } catch {socket.close(1008,'invalid_tool_event');}
      });
      socket.on('error',()=>socket.terminate());
      socket.on('close',()=>{
        clearTimeout(configured);
        if (mac===socket) {mac=null; macReady=false; upstream.detachTools();}
      });
    };
    const cloudTools=lease && sessionStore.tools?.open(mediaID,lease.id,attachTools);
    const deadline = setTimeout(() => stop('connect_timeout'), 12000);
    // 35 minutes provides room for the requested 30-minute soak while bounding spend.
    const duration = setTimeout(() => stop('duration_limit'), 35 * 60 * 1000);
    const stop = reason => {
      if (closing) return; closing = true;
      lastSession = {id: '', expires: 0};
      sendClient({type: 'gateway.error', error: {code: reason, retryable: ['connect_timeout', 'upstream_unavailable', 'slow_upstream', 'audio_rate_limit'].includes(reason)}});
      sendUp({type: 'session.close'});
      const timer = setTimeout(() => {upstream.terminate(); client.close(1011, reason);}, 1000);
      timer.unref();
    };
    if (lease) {
      // A 45s lease, renewed every 15s, expires after a killed instance without freeing a live paid call.
      renewal = setInterval(async () => {
        if (renewing || releasing) return;
        renewing = true;
        try { if (!await sessionStore.renew(lease.id)) stop('session_lease_lost'); }
        catch { stop('session_store_unavailable'); }
        finally { renewing = false; }
      },15000);
      const connectedAt = now();
      rotation = setInterval(() => {
        if (closing) return;
        const age = now()-connectedAt;
        // Hobby Functions allow 300s. Rotate after 240s at a quiet turn boundary; leave 20s for cleanup.
        if (age >= 240000 && ready && !outputActive && !truncation && !userSpeaking && now()-lastUserActivity >= 1500) {
          rotating = true; closing = true; sendUp({type:'session.close'});
        } else if (age >= 280000) stop('duration_limit');
      },1000);
    }
    upstream.on('open', () => {
      const config = doubaoSession(env.DOUBAO_VOICE || 'zh_female_vv_jupiter_bigtts');
      if (resume) config.session.id = resume;
      sendUp(config);
    });
    upstream.on('message', (raw, binary) => {
      try {
        let event;
        if (legacy) {
          if (!binary) throw new Error('expected_binary');
          const frame = decodeDoubao2(raw);
          if (frame.event >= 100 && frame.id !== wireSession) throw new Error('wrong_session');
          if (frame.event === 50) {
            if (connectionStarted) throw new Error('duplicate_connection_started');
            connectionStarted = true;
            if (!closing) sendUp({type: 'session.start'}); return;
          }
          event = doubao2Event(frame, legacyState);
          if (!event) return;
        } else {
          if (binary) throw new Error('expected_json');
          event = JSON.parse(raw.toString());
        }
        if (typeof event.type !== 'string') throw new Error('missing_type');
        // The Plan ASR emits its final uploaded duration while closing. Keep accounting
        // before the close ACK while continuing to reject late speech and audio.
        if (closing && !['session.closed', 'gateway.usage'].includes(event.type)) return;
        if (event.type === 'session.created') {
          if (ready || typeof event.session?.id !== 'string' || !event.session.id) throw new Error('invalid_session');
          ready = true; clearTimeout(deadline);
          lastSession = {id: event.session.id, expires: now() + 30*60000};
          if (plan) {event.call_id=mediaID; event.capabilities={task_tools:true,reports:true};}
        }
        if (event.type.startsWith('conversation.item.input_audio_transcription.')) {
          lastUserActivity = now();
          if (event.type.endsWith('.started')) userSpeaking = true;
          if (event.type.endsWith('.completed') || event.type.endsWith('.failed')) userSpeaking = false;
          if (macReady) {
            if (event.type.endsWith('.started')) sendMac({type:'input_audio_buffer.speech_started',item_id:event.item_id});
            if (event.type.endsWith('.completed')) {
              sendMac({type:'input_audio_buffer.speech_stopped',item_id:event.item_id});
              sendMac(event);
            }
          }
        }
        if (plan && ['response.created','response.done'].includes(event.type)) {if (macReady) sendMac(event); return;}
        if (event.type === 'session.closed') {
          closing = true;
          if (!rotating) closingEvent = event;
          upstream.close(); return;
        }
        if (event.type === 'gateway.context.truncated') {
          if (!truncation) throw new Error('unexpected_truncation_ack');
          clearTimeout(truncateTimer); truncation = null;
          outputActive = false; activeResponse = ''; outputBytes = 0;
          for (const audio of queuedAudio) sendUp({type: 'input_audio_buffer.append', audio});
          queuedAudio = []; sendClient(event); return;
        }
        if (event.type === 'gateway.behavior' && (!plan || userSpeaking || truncation ||
            event.response_id !== activeResponse || blocked.has(event.response_id))) return;
        if (event.type.startsWith('response.output_')) {
          const id = event.response_id;
          // The official 3.0 started example has an empty response_id; the first delta supplies it.
          if (!legacy && event.type === 'response.output_audio.started' && id === '') {
            outputActive = true; sendClient(event); return;
          }
          if (typeof id !== 'string' || !id) throw new Error('missing_response_id');
          if (blocked.has(id)) return;
          // Never present a new reply generated before the old history was corrected.
          if (truncation) return stop('context_sync_race');
          if (id !== activeResponse) {
            if (outputActive && activeResponse) return stop('overlapping_output');
            activeResponse = id; outputBytes = 0;
          }
          outputActive = true;
          if (event.type === 'response.output_audio.delta') {
            if (typeof event.delta !== 'string' || event.delta.length > 640000) throw new Error('bad_audio');
            const pcm = Buffer.from(event.delta, 'base64');
            // Matches the native enqueue ceiling: 24000 × 2 × 10 s = 480000 bytes.
            if (!pcm.length || pcm.length % 2 || pcm.toString('base64') !== event.delta) throw new Error('bad_audio');
            outputBytes += pcm.length;
          }
        }
        if (event.type === 'response.function_call_arguments.done') {
          for (const item of event.items || []) if (typeof item.call_id === 'string') pendingTools.add(item.call_id);
          if (pendingTools.size > 16) throw new Error('too_many_tools');
        }
        if (event.type === 'error') {
          // Never forward raw upstream diagnostics: they may echo credentials or transcripts.
          sendClient({type: 'gateway.error', error: {code: 'provider_error', retryable: String(event.error?.code || '').startsWith('5')}});
          stop('provider_error'); return;
        }
        sendClient(event);
      } catch {stop('invalid_provider_event');}
    });
    client.on('message', (raw, binary) => {
      try {
        if (!ready || closing || binary) throw new Error('session_not_ready');
        if (now() - audioWindow >= 1000) {audioWindow = now(); controls = 0;}
        const event = JSON.parse(raw.toString());
        if (event.type === 'input_audio_buffer.append') {
          // 16000 Hz × 2 bytes × .020 seconds = 640 bytes per capture frame.
          if (typeof event.audio !== 'string' || !/^[A-Za-z0-9+/]{854}==$/.test(event.audio) ||
              Buffer.byteLength(event.audio, 'base64') !== 640) throw new Error('invalid_audio');
          // PCM16/16k = 32 bytes/ms. Allow up to 3s / 96000 bytes of network jitter, below
          // the 128 KiB upstream ceiling; refill at the unchanged 50 frames/s rate.
          const current = now();
          audioBudget = Math.min(96000,audioBudget+Math.max(0,current-audioUpdated)*32); audioUpdated = current;
          if (audioBudget < 640) throw new Error('audio_rate_limit');
          audioBudget -= 640;
          // Capture and control send independently: discard in-flight frames after mute,
          // before upstream upload or truncation buffering; validation/rate limits still apply.
          if (muted) return;
          if (truncation) {
            // Plan subtitles may trail audio: 150 × 20 ms = 3 seconds / 96000 bytes; 2.0 keeps 1 second.
            if (queuedAudio.length >= (plan ? 150 : 50)) throw new Error('truncate_audio_overflow');
            queuedAudio.push(event.audio); return;
          }
          sendUp({type: event.type, audio: event.audio}); return;
        }
        if (++controls > 30) throw new Error('control_rate_limit');
        if (plan && event.type==='gateway.tools.request') {
          if (typeof event.request_id!=='string' || !/^[A-Za-z0-9_-]{1,128}$/.test(event.request_id)) throw Error('invalid_ticket_request');
          toolTickets.delete(toolToken); toolToken=randomBytes(32).toString('base64url');
          const url=new URL('v1/tools',origin); url.protocol=origin.protocol==='https:' ? 'wss:' : 'ws:';
          const token=toolToken;
          const deliver=()=>{if (!closing && token===toolToken) sendClient({type:'gateway.tools.ticket',request_id:event.request_id,call_id:mediaID,
            url:url.href,token,expires_at:Math.floor(now()/1000)+30});};
          if (cloudTools) void cloudTools.mint(token).then(deliver).catch(()=>stop('tool_store_unavailable'));
          else {toolTickets.set(token,{id:mediaID,expires:now()+30000,attach:attachTools});deliver();}
          return;
        }
        if (plan && event.type==='gateway.report') {
          if (typeof event.report_id!=='string' || !/^[A-Za-z0-9_-]{1,128}$/.test(event.report_id) ||
            typeof event.text!=='string' || !event.text.trim() || [...event.text].length>600) throw Error('invalid_report');
          if (userSpeaking || outputActive || truncation || !upstream.report({id:event.report_id,text:event.text})) {
            sendClient({type:'gateway.report.deferred',report_id:event.report_id});
          }
          return;
        }
        if (event.type === 'input_audio_mute.commit' || event.type === 'input_audio_unmute.commit') {
          muted = event.type === 'input_audio_mute.commit';
          if (muted) queuedAudio = [];
          sendUp({type: event.type}); return;
        }
        if (event.type === 'session.close') {
          closing = true;
          // Bound paid-session teardown even if the provider never acknowledges closure.
          closeTimer = setTimeout(() => {upstream.terminate(); client.close(1000, 'close_timeout');}, 1000);
          closeTimer.unref(); sendUp({type: event.type}); return;
        }
        if (event.type === 'gateway.interrupt') {
          if (!activeResponse || event.response_id !== activeResponse || blocked.has(activeResponse)) return;
          // 24 kHz × 2 bytes / 1000 = 48 bytes/ms. Received duration is only an upper bound.
          if (!Number.isFinite(event.played_ms) || event.played_ms < 0 || event.played_ms > outputBytes / 48) throw new Error('invalid_playback_progress');
          blocked.add(activeResponse); if (blocked.size > 64) blocked.delete(blocked.values().next().value);
          if (!legacy && !plan) {
            // 3.0 does not establish response_id == assistant item ID or audio-time truncation.
            // End this history instead of silently letting the next turn use unheard output.
            sendUp({type: 'response.cancel'}); stop('context_truncation_unavailable'); return;
          }
          truncation = activeResponse;
          // A local Plan adapter can acknowledge synchronously; arm the timeout before dispatch.
          truncateTimer = setTimeout(() => stop('context_sync_timeout'), plan ? 3000 : 1000);
          // In keep_alive mode server VAD cancels generation; ClientInterrupt(515) is push_to_talk only.
          sendUp({type: 'conversation.item.truncate', id: activeResponse, played_ms: Math.floor(event.played_ms)});
          return;
        }
        if (event.type === 'gateway.proactive') {
          if (muted || userSpeaking || outputActive || now()-lastUserActivity < 45000 || now()-lastProactive < 120000) return;
          lastProactive = now();
          // Use documented greeting TTS; autonomous topic generation is a later live-verified step.
          sendUp({type: 'speech_text_buffer.commit', text: '我在这里。你想聊些什么，或者安静待一会儿也好。'}); return;
        }
        if (event.type === 'gateway.playback.done') {
          if (!truncation && event.response_id === activeResponse && !blocked.has(activeResponse)) {
            const completed = activeResponse;
            blocked.add(activeResponse); if (blocked.size > 64) blocked.delete(blocked.values().next().value);
            outputActive = false; activeResponse = ''; lastUserActivity = now();
            if (plan) sendUp({type:event.type,response_id:completed});
          }
          return;
        }
        if (event.type === 'conversation.item.create' && Array.isArray(event.items) && event.items.length <= 8) {
          const items = event.items.map(item => {
            if (!pendingTools.has(item.call_id)) throw new Error('unknown_tool');
            pendingTools.delete(item.call_id);
            const content = item.content?.[0]?.text;
            if (item.role !== 'tool' || typeof content !== 'string' || content.length > 512) throw new Error('invalid_tool_result');
            return {call_id: item.call_id, role: 'tool', content: [{type: 'input_text', text: content}]};
          });
          sendUp({type: event.type, items}); return;
        }
        throw new Error('unsupported_event');
      } catch(error) {
        const known=['session_not_ready','invalid_audio','audio_rate_limit','truncate_audio_overflow','control_rate_limit','invalid_ticket_request','invalid_report','invalid_playback_progress','unknown_tool','invalid_tool_result','unsupported_event'];
        if (!closing) console.warn('voice_client_rejected',known.includes(error.message)?error.message:'invalid_json');
        stop(error.message==='audio_rate_limit' ? 'audio_rate_limit' : 'invalid_client_event');
      }
    });
    upstream.on('error', () => stop('upstream_unavailable'));
    upstream.on('close', () => {
      // The provider is gone. Let bounded cloud history/lease writes finish before sending the ACK.
      clearTimeout(closeTimer);
      closeTools();
      // An incomplete playback must not silently resume a generated-but-unheard history.
      if (outputActive || truncation) lastSession = {id: '', expires: 0};
      clearTimeout(deadline); clearTimeout(duration); clearTimeout(truncateTimer);
      queuedAudio = [];
      void release().finally(()=>{
        if (closingEvent) sendClient(closingEvent);
        client.close(rotating ? 1012 : closing ? 1000 : 1011, rotating ? 'service_rotation' : 'upstream_closed');
      });
    });
    client.on('error', () => stop('client_unavailable'));
    client.on('close', () => {
      closeTools();
      if (outputActive || truncation) lastSession = {id: '', expires: 0};
      clearTimeout(deadline); clearTimeout(duration); clearTimeout(truncateTimer); queuedAudio = [];
      if (upstream.readyState === WebSocket.OPEN) {
        sendUp({type: 'session.close'});
        const timer = setTimeout(() => upstream.terminate(), 1000); timer.unref();
      } else upstream.terminate();
    });
  }
  server.on('close', () => {
    collaborationRelay.close();
    for (const ws of wss.clients) ws.terminate(); wss.close();
    for (const ws of taskWss.clients) ws.terminate(); taskWss.close(); toolTickets.clear();
  });
  return server;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const server = makeGateway({fetchImpl:fetchPlan});
  server.listen(Number(process.env.PORT || 8787), process.env.HOST || '127.0.0.1', () => console.log('Norma voice gateway ready'));
}
