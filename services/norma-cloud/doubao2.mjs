import {gunzipSync} from 'node:zlib';

// Official 2.0 protocol: https://docs.volcengine.com/docs/6561/1594356
// 3.0 uses a different endpoint and does not document ConversationTruncate.
export function encodeDoubao2(event, payload = {}, sessionID = '') {
  const audio = Buffer.isBuffer(payload);
  const body = audio ? payload : Buffer.from(JSON.stringify(payload));
  const id = Buffer.from(sessionID);
  if (event >= 100 && !id.length) throw new Error('session_id_required');
  const frame = Buffer.alloc(12 + (event >= 100 ? 4 + id.length : 0) + body.length);
  frame.set([0x11, audio ? 0x24 : 0x14, audio ? 0 : 0x10, 0]);
  frame.writeUInt32BE(event, 4);
  let offset = 8;
  if (event >= 100) {
    frame.writeUInt32BE(id.length, offset); offset += 4;
    id.copy(frame, offset); offset += id.length;
  }
  frame.writeUInt32BE(body.length, offset); body.copy(frame, offset + 4);
  return frame;
}

export function decodeDoubao2(raw) {
  if (!Buffer.isBuffer(raw) || raw.length < 12 || raw[0] !== 0x11) throw new Error('invalid_header');
  const type = raw[1] >> 4, flags = raw[1] & 15, serialization = raw[2] >> 4, compression = raw[2] & 15;
  if (![9, 11, 15].includes(type) || flags & 8 || serialization > 1 || compression > 1) throw new Error('unsupported_header');
  let offset = 4;
  const integer = () => {
    if (offset + 4 > raw.length) throw new Error('short_frame');
    const value = raw.readUInt32BE(offset); offset += 4; return value;
  };
  const bytes = length => {
    if (length > raw.length - offset) throw new Error('short_frame');
    const value = raw.subarray(offset, offset + length); offset += length; return value;
  };
  const code = type === 15 ? integer() : 0;
  if (flags & 1) integer(); // Positive/negative sequence precedes event and session ID.
  const event = flags & 4 ? integer() : 0;
  const id = event >= 50 ? bytes(integer()).toString('utf8') : '';
  let body = bytes(integer());
  if (offset !== raw.length) throw new Error('trailing_bytes');
  if (compression) body = gunzipSync(body, {maxOutputLength: 2 * 1024 * 1024});
  if (body.length > 2 * 1024 * 1024) throw new Error('payload_too_large');
  if (serialization) {
    body = JSON.parse(body.toString());
    if (!body || typeof body !== 'object' || Array.isArray(body)) throw new Error('invalid_json');
  } else if (type !== 11 || event !== 352) throw new Error('unexpected_raw_payload');
  if (type === 15) throw new Error('provider_error');
  if (!event || (event === 352) !== Buffer.isBuffer(body)) throw new Error('invalid_event');
  return {event, id, payload: body, code};
}

export function doubao2Session(voice, instructions, resume = '') {
  return {
    asr: {extra: {enable_custom_vad: true, end_smooth_window_ms: 500}},
    tts: {speaker: voice, audio_config: {channel: 1, format: 'pcm_s16le', sample_rate: 24000}, extra: {}},
    dialog: {bot_name: 'Milltina', system_role: instructions, ...(resume ? {dialog_id: resume} : {}),
      extra: {model: '1.2.1.1', input_mod: 'keep_alive', enable_conversation_truncate: true}},
  };
}

// Keep the iPhone's PCM/caption messages stable across the two documented protocols.
export function doubao2Event({event, payload: p}, state) {
  switch (event) {
    case 150: return {type: 'session.created', session: {id: p.dialog_id}};
    case 152: return {type: 'session.closed'};
    case 51: case 153: case 599: throw new Error('provider_error');
    case 350:
      if (typeof p.reply_id !== 'string' || !p.reply_id) throw new Error('missing_reply_id');
      state.reply = p.reply_id;
      return {type: 'response.output_audio.started', response_id: state.reply};
    case 352:
      if (!state.reply || !p.length || p.length % 2) throw new Error('invalid_audio');
      return {type: 'response.output_audio.delta', response_id: state.reply, delta: p.toString('base64')};
    case 359: return {type: 'response.output_audio.done', response_id: p.reply_id};
    case 450:
      state.transcript = '';
      return {type: 'conversation.item.input_audio_transcription.started'};
    case 451:
      if (!Array.isArray(p.results) || p.results.some(r => typeof r.text !== 'string')) throw new Error('invalid_transcript');
      state.transcript = p.results.map(r => r.text).join('');
      return {type: 'conversation.item.input_audio_transcription.delta', transcript: state.transcript};
    case 459: return {type: 'conversation.item.input_audio_transcription.completed', transcript: state.transcript || ''};
    case 550: return {type: 'response.output_text.delta', response_id: p.reply_id, delta: p.content};
    case 154: return {type: 'response.done', usage: p.usage};
    case 570:
      if (p.message || p.error || p.status_code) throw new Error('truncate_failed');
      return {type: 'gateway.context.truncated'};
    default: return null;
  }
}
