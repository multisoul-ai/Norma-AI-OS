import {EventEmitter} from 'node:events';
import {randomUUID} from 'node:crypto';
import {gzipSync, gunzipSync} from 'node:zlib';
import {request as httpsRequest} from 'node:https';
import {Readable} from 'node:stream';
import WebSocket from 'ws';

// Native HTTPS avoids the cloud fetch connection stalls observed against Plan services.
// Keep streaming and the caller's abort signal; https.request never follows redirects.
export function fetchPlan(url, options) {
  return new Promise((resolve,reject)=>{
    const request=httpsRequest(url,options,response=>{
      try {resolve(new Response(Readable.toWeb(response),{status:response.statusCode,headers:response.headers}));}
      catch(error) {response.destroy();reject(error);}
    });
    request.on('error',reject);request.end(options.body);
  });
}

export function encodeASR(type, sequence, value) {
  const payload = gzipSync(Buffer.isBuffer(value) ? value : Buffer.from(JSON.stringify(value)));
  const size = Buffer.alloc(8); size.writeInt32BE(sequence); size.writeUInt32BE(payload.length,4);
  return Buffer.concat([Buffer.from([0x11,type,0x11,0]),size,payload]);
}

export function decodeASR(raw) {
  if (raw.length < 8 || raw[0]>>4 !== 1 || (raw[0]&15) < 1) throw new Error('invalid_asr_header');
  const type = raw[1]>>4, flags = raw[1]&15, compression = raw[2]&15;
  let offset = (raw[0]&15)*4, code;
  if (flags&1) offset += 4;
  if (flags&4) offset += 4;
  if (type === 15) {code = raw.readUInt32BE(offset); offset += 4;}
  else if (type !== 9) throw new Error('invalid_asr_type');
  const size = raw.readUInt32BE(offset); offset += 4;
  if (offset+size !== raw.length || raw[2]>>4 !== 1 || ![0,1].includes(compression)) throw new Error('invalid_asr_payload');
  const payload = JSON.parse((compression ? gunzipSync(raw.subarray(offset),{maxOutputLength:2*1024*1024}) : raw.subarray(offset)).toString());
  if (type === 15 || (payload.code && ![1000,20000000].includes(payload.code))) throw new Error(`asr_rejected_${code || payload.code}`);
  return {payload, last:!!(flags&2)};
}

function echoText(text) {
  return text.normalize('NFKC').toLowerCase().replace(/[零〇一二两三四五六七八九十百千万]+/gu, value => {
    const digits = {零:0,〇:0,一:1,二:2,两:2,三:3,四:4,五:5,六:6,七:7,八:8,九:9};
    if (!/[十百千万]/u.test(value)) return [...value].map(c=>digits[c]).join('');
    let total = 0, section = 0, digit = 0;
    for (const c of value) {
      if (c in digits) digit = digits[c];
      else if (c === '万') {total += (section+digit || 1)*10000; section = digit = 0;}
      else {section += (digit || 1)*({十:10,百:100,千:1000}[c]); digit = 0;}
    }
    return String(total+section+digit);
  }).replace(/[^\p{L}\p{N}]/gu,'');
}

function matchesEcho(candidate, reference) {
  if (!reference) return false;
  if (reference.includes(candidate)) return true;
  // ponytail: bounded transcript fallback after native AEC; similar simultaneous repetitions
  // remain ambiguous. Acoustic speaker separation is required to distinguish those cases.
  // Require >=6 characters, tolerate <=1 error per 8 (at most 2); short commands stay exact-only.
  if (candidate.length < 6) return false;
  const budget = Math.min(2, Math.max(1, Math.floor(candidate.length/8)));
  if (candidate.length > reference.length+budget) return false;
  // A changed number or negation is meaning, not an ASR spelling variation.
  const cost = c => /[0-9不没无别勿停等]/u.test(c) ? budget+1 : 1;
  let row = new Array(reference.length+1).fill(0);
  for (let i=0; i<candidate.length; i++) {
    const next = [row[0]+cost(candidate[i])];
    for (let j=0; j<reference.length; j++) next.push(Math.min(next[j]+cost(reference[j]),row[j+1]+cost(candidate[i]),
      row[j]+(candidate[i]===reference[j]?0:Math.max(cost(candidate[i]),cost(reference[j])))));
    row = next;
  }
  return Math.min(...row) <= budget;
}

// Adapts the three Plan services to the gateway's existing event/transport contract.
// History belongs to this personal gateway; a new session ID discards an unsafe resume.
export function createPlanUpstream({key, fetchImpl = fetch, asrFactory = (url, options) => new WebSocket(url, options), conversation, now = Date.now}) {
  const up = new EventEmitter(); up.readyState = WebSocket.CONNECTING; up.bufferedAmount = 0;
  let closed = false, configured = false, muted = false, started = false, speaking = false;
  let asr, asrReady = false, deadline, frames = [], sequence = 2, lastFinal = -1, finalText = '';
  let instructions = '', behaviorOptions, pending = '', reply = null, taskTools = [], taskInstructions = '';
  let turnID = '';
  let echoes = [];
  // 500 ms VAD + two 240 ms audio batches + 520 ms transport margin.
  const echoTailMS = 1500;
  // Sending the last network chunk is not the end of speaker playback (up to 6 s buffered).
  const echoActive = e => (reply?.id === e.id && !reply.interrupted) || now() <= e.until;
  const emit = event => {if (!closed) up.emit('message', Buffer.from(JSON.stringify(event)), false);};
  const fail = () => {if (!closed) up.emit('error',new Error('plan_service_failed'));};
  const appendHistory = (role, content) => {
    if (!content) return;
    conversation.messages.push({role,content});
    // Same ceiling as native text recovery: 24 messages and 32 KiB, keeping the newest turn.
    while (conversation.messages.length > 24 || Buffer.byteLength(JSON.stringify(conversation.messages)) > 32768) conversation.messages.shift();
  };
  function endASR() {
    clearTimeout(deadline);
    if (asrReady && sequence > 2) emit({type:'gateway.usage',usage:{service:'plan_asr',uploaded_audio_seconds:(sequence-2)*0.2}});
    const old = asr; asr = null; asrReady = false; frames = []; up.bufferedAmount = 0;
    old?.terminate();
  }
  function connectASR() {
    endASR(); sequence = 2; lastFinal = -1; finalText = '';
    const peer = asrFactory('wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async',{
      headers:{'X-Api-Key':key,'X-Api-Resource-Id':'volc.seedasr.sauc.duration','X-Api-Request-Id':randomUUID()},
      handshakeTimeout:10000,maxPayload:2*1024*1024,perMessageDeflate:false,
    });
    asr = peer;
    deadline = setTimeout(fail,10000);
    peer.on('open', () => {
      if (asr !== peer || closed) return;
      peer.send(encodeASR(0x11,1,{audio:{format:'pcm',codec:'raw',rate:16000,bits:16,channel:1},
        request:{model_name:'bigmodel',show_utterances:true,enable_nonstream:true,result_type:'single',end_window_size:500}}));
    }); // 500 ms is inside the documented [300,5000] VAD window; tune against real speech pauses.
    peer.on('message',(raw,binary) => {
      if (asr !== peer || closed || muted) return;
      try {
        if (!binary) throw new Error('expected_asr_binary');
        const {payload,last} = decodeASR(raw);
        if (last) throw new Error('unexpected_asr_end');
        if (!asrReady) {
          asrReady = true; clearTimeout(deadline);
          if (!started) {started = true; emit({type:'session.created',session:{id:conversation.id}});}
          flushAudio();
        }
        const utterances = payload.result?.utterances || [];
        if (!Array.isArray(utterances) || utterances.length > 128) throw new Error('invalid_utterances');
        const fresh = utterances.filter(u => typeof u.text === 'string' && u.text.trim() && Number.isFinite(u.end_time) && u.end_time > lastFinal).sort((a,b)=>a.end_time-b.end_time);
        if (!fresh.length) return;
        let finalEnd = lastFinal, committed = '';
        for (const u of fresh) if (u.definite === true && u.end_time > finalEnd) {committed += u.text; finalEnd = u.end_time;}
        const interim = fresh.filter(u => u.definite !== true && u.end_time > finalEnd).map(u => u.text).join('');
        const transcript = finalText+committed+interim;
        if (transcript.length > 2000) throw new Error('utterance_too_long');
        echoes = echoes.filter(echoActive);
        const candidate = echoText(transcript);
        // Residual speaker echo can survive native AEC. Only match recently emitted speech;
        // once genuine user speech starts, its continuation always takes priority.
        if (!speaking && candidate && echoes.some(e => e.text.some(text=>matchesEcho(candidate,text)))) {
          lastFinal = finalEnd; return;
        }
        if (!speaking) {
          speaking = true;
          turnID = randomUUID();
          if (reply) {
            reply.interrupted = true; reply.audio = Buffer.alloc(0); clearInterval(reply.timer);
            if (!reply.ttsStarted) {reply.controller.abort(); reply.settled = true;}
          }
          emit({type:'conversation.item.input_audio_transcription.started',item_id:turnID});
        }
        finalText += committed; lastFinal = finalEnd;
        emit({type:'conversation.item.input_audio_transcription.delta',transcript});
        if (!interim && finalText) {
          speaking = false; pending += finalText; finalText = '';
          emit({type:'conversation.item.input_audio_transcription.completed',item_id:turnID,transcript:pending});
          pump();
        }
      } catch {fail();}
    });
    peer.on('error', () => {if (asr === peer && !closed) fail();});
    peer.on('close', () => {if (asr === peer && !closed) fail();});
  }
  function flushAudio() {
    if (!asrReady || !asr || muted) return;
    while (frames.length >= 10) {
      if (asr.bufferedAmount > 128*1024) throw new Error('asr_backpressure');
      // Ten native 20 ms frames = 6400 bytes / 200 ms, matching the official stream sample.
      asr.send(encodeASR(0x21,sequence++,Buffer.concat(frames.splice(0,10))));
    }
    up.bufferedAmount = frames.length*640;
  }
  async function request(url, headers, body, signal, onRecord) {
    const response = await fetchImpl(url,{method:'POST',redirect:'error',
      signal:AbortSignal.any([signal,AbortSignal.timeout(20000)]),
      headers:{...headers,'Content-Type':'application/json'},body:JSON.stringify(body)});
    if (!response.ok) {await response.body?.cancel(); throw new Error('plan_http_rejected');}
    const decoder = new TextDecoder('utf-8',{fatal:true});
    let buffer = '', bytes = 0;
    for await (const chunk of response.body) {
      bytes += chunk.length;
      // PCM16 × 24 kHz × 30 s = 1.44 MB; Base64 plus metadata comfortably fits 3 MiB.
      if (bytes > 3*1024*1024) throw new Error('plan_response_too_large');
      buffer += decoder.decode(chunk,{stream:true});
      if (onRecord) {
        let end;
        while ((end = buffer.indexOf('\n')) !== -1) {
          const line = buffer.slice(0,end); buffer = buffer.slice(end+1);
          if (line.trim()) onRecord(JSON.parse(line));
        }
      }
    }
    buffer += decoder.decode();
    if (onRecord) {if (buffer.trim()) onRecord(JSON.parse(buffer));}
    else return JSON.parse(buffer);
  }
  function tick(r) {
    if (closed || reply !== r || r.interrupted) return;
    if (r.audio.length) {
      if (r.behavior) {
        emit({type:'gateway.behavior',response_id:r.id,name:'set_behavior',arguments:JSON.stringify(r.behavior)});
        r.behavior = null;
      }
      const chunk = r.audio.subarray(0,4800); r.audio = r.audio.subarray(chunk.length);
      echoes = [...echoes.filter(e => e.id !== r.id && echoActive(e)),
        {id:r.id,text:[r.echoText,echoText(r.text)],until:now()+echoTailMS}].slice(-2);
      emit({type:'response.output_audio.delta',response_id:r.id,delta:chunk.toString('base64')});
    } else if (r.settled) {
      clearInterval(r.timer); r.outputDone = true;
      emit({type:'response.output_audio.done',response_id:r.id});
    }
  }
  function truncate(r) {
    if (reply !== r || r.played === undefined) return;
    if (r.played > 0 && !r.settled) return; // Timestamps can arrive after the first audio chunks.
    if (r.played > 0 && !r.words.length) {fail(); return;}
    const heard = r.words.filter(w => w.endTime*1000 <= r.played).map(w => w.word).join('');
    if (!r.report) appendHistory('assistant',heard);
    r.controller.abort(); clearInterval(r.timer); reply = null;
    emit({type:'gateway.context.truncated'});
    pump();
  }
  function pump(proactive = false, report = null) {
    if (closed || (!report && muted) || speaking || reply || (!pending && !proactive && !report)) return false;
    if (!report) {const userText = pending; pending = ''; appendHistory('user',userText);}
    const r = {id:randomUUID(),controller:new AbortController(),audio:Buffer.alloc(0),words:[],text:'',
      bytes:0,settled:false,interrupted:false,ttsStarted:false,outputDone:false,proactive,report};
    reply = r;
    emit({type:'response.output_audio.started',response_id:r.id,...(report ? {report_id:report.id} : {})});
    void respond(r).catch(() => {if (!closed && reply === r && !r.controller.signal.aborted) fail();});
    return true;
  }
  async function respond(r) {
    let result = r.report ? {text:r.report.text} : null;
    const messages = [...conversation.messages];
    // Mac owns the authoritative 20-call budget. One extra round can speak its rejection.
    for (let round=0; !result && round<=20; round++) {
      const tools = r.unknown ? [] : taskTools;
      const data = await request('https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions',{Authorization:`Bearer ${key}`},
      {model:'doubao-seed-2.0-mini',messages:[{role:'system',content:instructions+' 只输出JSON对象：text为一句最多40字的中文回复；emotion只能为'+
        behaviorOptions.emotion.enum.join('、')+'；gesture只能为'+behaviorOptions.gesture.enum.join('、')+'。每次按语义选择表情：开心用happy，好奇或提问用curious，安慰担忧用concerned，平常用neutral。手势通常用none，只有问候告别才用wave，明确赞同时才用nod；不要每句话都挥手。不要Markdown或表情符号。'+
        (r.proactive ? ' 用户安静了一会儿，请温和地邀请聊天，也允许继续安静待着。' : '')+taskInstructions+
        (!tools.length ? ' 当前Mac工具不可用，不得声称已派工。' : '')},...messages],
        ...(tools.length ? {tools:tools.map(tool=>({type:'function',function:{name:tool.name,description:tool.description,parameters:tool.parameters}})),parallel_tool_calls:false} : {}),
        max_tokens:tools.length ? 1024 : 128,thinking:{type:'disabled'},response_format:{type:'json_object'}},r.controller.signal);
      if (closed || reply !== r || r.interrupted) return;
      emit({type:'gateway.usage',usage:{service:'plan_chat',...data.usage}});
      const choice = data.choices?.[0], message = choice?.message;
      if (choice?.finish_reason === 'tool_calls') {
        const calls = message?.tool_calls;
        if (!tools.length || !Array.isArray(calls) || !calls.length || calls.length > 20 ||
          calls.some(call=>call.type!=='function' || !/^[A-Za-z0-9_-]{1,128}$/.test(call.id) ||
            typeof call.function?.arguments!=='string' || call.function.arguments.length>32768 ||
            !tools.some(tool=>tool.name===call.function.name)) || new Set(calls.map(call=>call.id)).size!==calls.length) throw Error('invalid_tool_calls');
        messages.push({role:'assistant',content:message.content || null,tool_calls:calls});
        const outputs = await new Promise(resolve => {
          const finish = () => {
            if (!r.toolWait) return;
            const outputs = calls.map(call=>({role:'tool',tool_call_id:call.id,
              content:r.toolWait.results.get(call.id) || '{"status":"unknown","message":"Mac回执未核对，禁止再次提交"}'}));
            r.unknown = outputs.some(output=>JSON.parse(output.content).status==='unknown');
            clearTimeout(r.toolWait.timer); r.controller.signal.removeEventListener('abort',finish);
            r.toolWait=null; resolve(outputs);
          };
          // Mac's 30s dispatch lease + 5s receipt readback + 5s transport margin; no write retry.
          r.toolWait={calls,results:new Map(),finish,timer:setTimeout(finish,40000)};
          r.controller.signal.addEventListener('abort',finish,{once:true});
          const id=r.id+'_'+round;
          emit({type:'response.created',response:{id}});
          emit({type:'response.done',response:{id,status:'completed',output:calls.map(call=>({
            type:'function_call',call_id:call.id,name:call.function.name,arguments:call.function.arguments}))}});
        });
        messages.push(...outputs);
      } else {
        if (choice?.finish_reason !== 'stop') throw Error('invalid_plan_reply');
        result=JSON.parse(message?.content || 'null');
        if (!result) throw Error('invalid_plan_reply');
      }
    }
    if (closed || reply !== r || r.interrupted) return;
    const text = result?.text;
    if (typeof text !== 'string' || !text.trim() || [...text].length > (r.report ? 600 : 160)) throw new Error('invalid_plan_reply');
    r.echoText = echoText(text);
    // Invalid motion metadata never executes and never prevents a valid spoken reply.
    if (behaviorOptions.emotion.enum.includes(result.emotion) && behaviorOptions.gesture.enum.includes(result.gesture)) {
      r.behavior = {emotion:result.emotion,gesture:result.gesture};
    }
    emit({type:'response.output_text.delta',response_id:r.id,delta:text});
    // One speech request per sentence gives each word timestamp an unambiguous PCM origin.
    const sentences = text.match(/[^。！？!?\n]+[。！？!?]?/gu) || [];
    if (!sentences.length || sentences.length > (r.report ? 12 : 4)) throw new Error('too_many_sentences');
    r.timer = setInterval(() => tick(r),100);
    for (const sentence of sentences) {
      if (r.interrupted) break;
      r.ttsStarted = true;
      const offset = r.bytes/48000;
      let terminal = false, words = [], normalized = '', count = 0, sentenceBytes = 0;
      await request('https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional',
        {'X-Api-Key':key,'X-Api-Resource-Id':'seed-tts-2.0','X-Api-Request-Id':randomUUID()},
        {req_params:{text:sentence,speaker:'zh_female_vv_uranus_bigtts',audio_params:{format:'pcm',sample_rate:24000,enable_subtitle:true}}},r.controller.signal,event => {
          if (closed || reply !== r) return;
          if (terminal || ++count > 512) throw new Error('invalid_tts_records');
          if (event.code === 20000000) {terminal = true; return;}
          if (event.code !== 0) throw new Error('tts_rejected');
          if (event.sentence) {
            if (typeof event.sentence.text !== 'string' || !Array.isArray(event.sentence.words)) throw new Error('invalid_tts_subtitle');
            normalized = event.sentence.text;
            if (event.sentence.words.length) words = event.sentence.words;
          }
          if (event.data) {
            if (typeof event.data !== 'string') throw new Error('invalid_tts_audio');
            const pcm = Buffer.from(event.data,'base64');
            if (!pcm.length || pcm.length%2 || pcm.toString('base64') !== event.data) throw new Error('invalid_tts_pcm');
            r.bytes += pcm.length; sentenceBytes += pcm.length;
            if (r.bytes > 1440000) throw new Error('reply_audio_too_long');
            // ponytail: one bounded 30-second reply buffer; use a chunk deque if longer replies are needed.
            if (!r.interrupted) r.audio = Buffer.concat([r.audio,pcm]);
          }
        });
      if (!terminal || !sentenceBytes || !normalized) throw new Error('incomplete_tts');
      emit({type:'gateway.usage',usage:{service:'plan_tts',input_characters:[...sentence].length,audio_seconds:sentenceBytes/48000}});
      let previous = 0;
      for (const word of words) {
        if (typeof word.word !== 'string' || !Number.isFinite(word.startTime) || !Number.isFinite(word.endTime) ||
            word.startTime < previous || word.endTime < word.startTime || word.endTime > sentenceBytes/48000+0.02) throw new Error('invalid_word_timing');
        previous = word.endTime;
        r.words.push({...word,startTime:word.startTime+offset,endTime:word.endTime+offset});
      }
      r.text += normalized;
      const echo = echoes.find(e=>e.id === r.id);
      if (echo) echo.text = [r.echoText,echoText(r.text)];
    }
    r.settled = true;
    if (r.interrupted) truncate(r);
    else emit({type:'response.output_text.done',response_id:r.id,text:r.text});
  }
  up.configureTools = session => {taskTools=session.tools.filter(tool=>tool.name!=='set_behavior'); taskInstructions=session.instructions;};
  up.detachTools = () => {taskTools=[]; taskInstructions=''; reply?.toolWait?.finish();};
  up.toolResult = item => {
    const wait=reply?.toolWait;
    if (!wait || !wait.calls.some(call=>call.id===item.call_id)) return;
    if (item.type!=='function_call_output' || typeof item.output!=='string' || item.output.length>262144) throw Error('invalid_tool_result');
    const value=JSON.parse(item.output);
    if (!['accepted','succeeded','rejected','unknown'].includes(value.status)) throw Error('invalid_tool_status');
    if (wait.results.has(item.call_id) && wait.results.get(item.call_id)!==item.output) throw Error('conflicting_tool_result');
    wait.results.set(item.call_id,item.output);
  };
  up.continueTools = () => {
    const wait=reply?.toolWait;
    if (wait && wait.results.size===wait.calls.length) wait.finish();
  };
  up.report = report => pump(false,report);
  up.send = raw => {
    try {
      const event = JSON.parse(raw);
      if (closed) return;
      switch (event.type) {
        case 'session.create':
          if (configured) throw new Error('duplicate_session');
          configured = true;
          if (!event.session.id || event.session.id !== conversation.id) {conversation.id = randomUUID(); conversation.messages = [];}
          instructions = event.session.instructions.replace('需要表达情绪或动作时可调用 set_behavior，避免每句话都调用。','');
          behaviorOptions = event.session.tools.find(tool=>tool.name==='set_behavior').parameters.properties;
          connectASR(); break;
        case 'input_audio_buffer.append':
          if (muted || frames.length >= 50) throw new Error('invalid_audio_state');
          frames.push(Buffer.from(event.audio,'base64')); flushAudio(); break;
        case 'input_audio_mute.commit':
          muted = true; speaking = false; pending = ''; echoes = []; endASR(); break;
        case 'input_audio_unmute.commit': muted = false; connectASR(); break;
        case 'conversation.item.truncate':
          if (!reply || reply.id !== event.id) throw new Error('unknown_reply');
          reply.interrupted = true; reply.played = event.played_ms; reply.audio = Buffer.alloc(0); clearInterval(reply.timer);
          truncate(reply); break;
        case 'gateway.playback.done':
          if (reply?.id === event.response_id && reply.outputDone && !reply.interrupted) {
            const echo = echoes.find(e => e.id === reply.id);
            if (echo) echo.until = now()+echoTailMS;
            if (!reply.report) appendHistory('assistant',reply.text); reply = null; pump();
          }
          break;
        case 'speech_text_buffer.commit':
          pump(true); break;
        case 'session.close': endASR(); emit({type:'session.closed'}); up.terminate(); break;
        default: throw new Error('unsupported_plan_event');
      }
    } catch {fail();}
  };
  up.terminate = up.close = () => {
    if (closed) return;
    endASR(); closed = true; up.readyState = WebSocket.CLOSED;
    reply?.controller.abort(); clearInterval(reply?.timer); reply = null; pending = '';
    queueMicrotask(() => up.emit('close'));
  };
  queueMicrotask(() => {if (!closed) {up.readyState = WebSocket.OPEN; up.emit('open');}});
  return up;
}
