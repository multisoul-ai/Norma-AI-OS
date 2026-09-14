// Explicit real-cloud gateway check with a generated PCM fixture, never a live microphone.
// Run: node --env-file=.env probe-plan-call.mjs
import {readFileSync, writeFileSync} from 'node:fs';
import {once} from 'node:events';
import {Agent} from 'node:https';
import WebSocket from 'ws';
import {makeGateway} from './server.mjs';

const remoteAgent=new Agent({proxyEnv:process.env});
const input = readFileSync(process.env.NORMA_PROBE_PCM);
if (!input.length || input.length%2 || input.length>320000) throw new Error('Expected 1–10 seconds PCM16/16k/mono');
const report = {source:'fixed_generated_audio',playback:'receiver_only_no_speaker',input_seconds:input.length/32000,events:[],chat_requests:[]};
const start = performance.now(), audio = [];
const interruptCheck = process.argv.includes('--interrupt');
const behaviorCheck = process.argv.includes('--behavior');
const remoteArgument = process.argv.find(value=>value.startsWith('--gateway='))?.slice('--gateway='.length);
let remoteOrigin;
if (remoteArgument) {
  const url = new URL(remoteArgument);
  if (url.protocol !== 'https:' || url.username || url.password || url.pathname !== '/' || url.search || url.hash) throw new Error('Expected HTTPS gateway origin');
  remoteOrigin = url.origin;
}
report.gateway = remoteOrigin || 'local_ephemeral_gateway';
report.interrupt_check = interruptCheck;
report.behavior_check = behaviorCheck;
const env = {...process.env,DOUBAO_PROTOCOL:'plan',PUBLIC_ORIGIN:'http://127.0.0.1:8787'};
const gateway = remoteOrigin ? null : makeGateway({env,fetchImpl:async (url,options) => {
  if (url.endsWith('/chat/completions')) report.chat_requests.push({at_ms:Math.round(performance.now()-start),
    messages:JSON.parse(options.body).messages.filter(m=>m.role!=='system')});
  return fetch(url,options);
}});
if (gateway) {gateway.listen(0,'127.0.0.1'); await once(gateway,'listening');}
let mac, reportStarted=false;
const deploymentCheck=process.argv.includes("--deployment");
let ws, timer, injected, failure, sent = 0, audioBytes = 0, inputOffset = 0, turn = 0, activeID = '', interruptedID = '';
// Deployment adds tool binding and a second TTS turn: 12s connect + 4s input + 3×20s provider budget + 4s close + 10s network margin = 90s.
const deadline = setTimeout(()=>{failure='probe_timeout';ws?.terminate();},deploymentCheck?90000:40000);
try {
  const base = remoteOrigin || `http://127.0.0.1:${gateway.address().port}`;
  if (deploymentCheck) {
    const anonymous=await fetch(`${base}/v1/session`,{method:'POST',signal:AbortSignal.timeout(10000),headers:{'Content-Type':'application/json'},body:'{"provider":"doubao"}'});
    if (anonymous.status!==401) throw Error('anonymous_session_not_rejected');
    report.anonymous_status=anonymous.status;
  }
  const response = await fetch(`${base}/v1/session`,{method:'POST',redirect:'error',signal:AbortSignal.timeout(10000),
    headers:{Authorization:`Bearer ${env.COMPANION_ACCESS_TOKEN}`,'Content-Type':'application/json'},body:'{"provider":"doubao"}'});
  if (!response.ok) throw new Error(`ticket_http_${response.status}`);
  const ticket = await response.json();
  const socketURL = new URL(remoteOrigin ? ticket.url : `${base.replace('http:','ws:')}/v1/voice`);
  if (remoteOrigin && (socketURL.protocol !== 'wss:' || socketURL.host !== new URL(base).host)) throw new Error('Unexpected gateway voice address');
  ws = new WebSocket(socketURL,{agent:remoteOrigin?remoteAgent:undefined,headers:{Authorization:`Bearer ${ticket.token}`},handshakeTimeout:10000});
  ws.on('error',()=>{failure='gateway_transport_failed';});
  const closed = once(ws,'close');
  ws.on('message',raw => {
    try {
      const event = JSON.parse(raw), at = Math.round(performance.now()-start);
      if (event.type !== 'response.output_audio.delta') {const safe={at_ms:at,...event};delete safe.token;report.events.push(safe);}
      if (report.events.length>256) throw new Error('too_many_events');
      if (event.type === 'gateway.tools.ticket' && deploymentCheck) {
        const toolsURL=new URL(event.url);toolsURL.searchParams.set('call_id',event.call_id);
        if (toolsURL.protocol!=='wss:' || toolsURL.host!==new URL(base).host) throw Error('unexpected_tools_origin');
        mac=new WebSocket(toolsURL,{agent:remoteAgent,headers:{Authorization:`Bearer ${event.token}`},handshakeTimeout:10000});
        mac.on('error',()=>{failure='tools_transport_failed';ws.terminate();});
        mac.on('message',raw=>{
          const control=JSON.parse(raw);
          if(control.type==='session.created') mac.send(JSON.stringify({type:'session.update',session:{type:'realtime',instructions:'你是Norma，请用一句话介绍自己。',tools:[]}}));
          if(control.type==='session.updated') report.tools_connected=true;
        });
      }
      if (event.type === 'session.created') {
        if(deploymentCheck) ws.send(JSON.stringify({type:'gateway.tools.request',request_id:'deployment-check'}));
        timer = setInterval(()=>{
          if (ws.readyState!==WebSocket.OPEN) return;
          // At most 75s of synthetic PCM (90s total minus initial connection/setup); only the first phrase is speech.
          if (sent>=(deploymentCheck?75:30)*32000) {failure='input_duration_limit';ws.terminate();return;}
          const frame = Buffer.alloc(640);
          if (inputOffset<input.length) input.copy(frame,0,inputOffset,Math.min(inputOffset+640,input.length));
          ws.send(JSON.stringify({type:'input_audio_buffer.append',audio:frame.toString('base64')})); sent+=640;
          inputOffset+=640;
        },20); // 16000 × 2 × 0.020 = 640 bytes, including silence after the fixed phrase for server VAD.
      }
      if (event.type === 'response.output_audio.started') {
        activeID = event.response_id; turn++; audio.length = 0; audioBytes = 0;
      }
      if (event.type === 'conversation.item.input_audio_transcription.started' && activeID && interruptCheck && !interruptedID) {
        // This models a receiver at 600 ms, not an iPhone speaker measurement.
        const played = Math.min(600,audioBytes/48); interruptedID = activeID;
        report.interrupt = {at_ms:at,response_id:activeID,played_ms:played};
        ws.send(JSON.stringify({type:'gateway.interrupt',response_id:activeID,played_ms:played}));
      }
      if (event.type === 'response.output_audio.delta') {
        if (event.response_id === interruptedID) throw new Error('late_interrupted_audio');
        const pcm = Buffer.from(event.delta,'base64'); audioBytes+=pcm.length;
        if (audioBytes>1440000) throw new Error('audio_duration_limit');
        audio.push(pcm); report.first_audio_ms ??= at;
        if (interruptCheck && !injected) injected = setTimeout(()=>{inputOffset=0;},100);
      }
      if (event.type === 'response.output_audio.done') {
        (report.audio_turns ??= []).push({response_id:event.response_id,report:reportStarted,audio_bytes:audioBytes});
        ws.send(JSON.stringify({type:'gateway.playback.done',response_id:event.response_id}));
        activeID = '';
        if (deploymentCheck && !reportStarted) {
          reportStarted=true;
          ws.send(JSON.stringify({type:'gateway.report',report_id:'deployment-report',text:'这是一段部署验证语音。'}));
          return;
        }
        if (!interruptCheck || turn>=2) {
          report.completed = !interruptCheck || !!report.interrupt && report.events.some(e=>e.type==='gateway.context.truncated');
          ws.send('{"type":"session.close"}');
        }
      }
      if (event.type === 'gateway.error') {failure=event.error.code;ws.terminate();}
    } catch {failure='invalid_probe_event';ws.terminate();}
  });
  await closed;
} catch(error) {failure=error.message;}
finally {
  clearTimeout(deadline); clearInterval(timer); clearTimeout(injected); ws?.terminate(); mac?.terminate(); remoteAgent.destroy();
  if (gateway) {gateway.closeAllConnections(); await new Promise(resolve=>gateway.close(resolve));}
}
report.audio_bytes = audioBytes; report.output_seconds = audioBytes/48000;
report.sent_input_seconds = sent/32000; report.elapsed_ms = Math.round(performance.now()-start);
if (behaviorCheck && report.completed) {
  const events = report.events.filter(e=>e.type==='gateway.behavior');
  report.valid_behavior = events.length > 0 && events.every(event=>{
    const value=JSON.parse(event.arguments);
    return event.name==='set_behavior' && ['neutral','happy','curious','concerned'].includes(value.emotion) && ['none','nod','wave'].includes(value.gesture);
  });
  if (!report.valid_behavior) failure='missing_or_invalid_behavior';
}
if(deploymentCheck && (!report.events.some(e=>e.type==='session.closed') || !report.tools_connected || !report.audio_turns?.some(t=>!t.report&&t.audio_bytes>0) ||
    !report.audio_turns?.some(t=>t.report&&t.audio_bytes>0) || !report.events.some(e=>e.type==='conversation.item.input_audio_transcription.completed'))) failure ??= 'deployment_evidence_incomplete';
if (failure) {report.error=failure;report.completed=false;}
const name = `${remoteOrigin?'vercel-':''}plan-call${behaviorCheck?'-behavior':''}${interruptCheck?'-interrupt':''}`;
if (report.completed) writeFileSync(`${process.env.NORMA_PROBE_OUTPUT}/${name}-output.pcm`,Buffer.concat(audio));
let output = JSON.stringify(report,null,2);
for (const name of ['COMPANION_ACCESS_TOKEN','DOUBAO_SPEECH_API_KEY']) if (env[name]) output=output.replaceAll(env[name],'[REDACTED]');
writeFileSync(`${process.env.NORMA_PROBE_OUTPUT}/${name}-result.json`,output+'\n'); console.log(output);
process.exitCode=report.completed?0:1;
