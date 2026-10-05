import {createHash, randomBytes, randomUUID, timingSafeEqual} from 'node:crypto';
import {closeSync, existsSync, fsyncSync, mkdirSync, openSync, readFileSync, renameSync, unlinkSync, writeFileSync} from 'node:fs';
import {dirname} from 'node:path';
import {pairedRegistration} from './paired-registration.mjs';

const routes = new Set(['/v1/admin/devices/invite', '/v1/admin/devices/list', '/v1/admin/devices/revoke',
  '/v1/devices/claim', '/v1/devices/register', '/v1/text/turn']);
const hash = value => createHash('sha256').update(value).digest('hex');
const uuid = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
const tokenFrom = req => req.headers.authorization?.match(/^Bearer ([A-Za-z0-9_-]+)$/)?.[1];
const json = (res, status, value, extra = {}) => {
  res.writeHead(status, {'Content-Type': 'application/json', 'Cache-Control': 'no-store', ...extra});
  res.end(JSON.stringify(value));
};

function sameSecret(actual, expected) {
  const a = Buffer.from(actual || ''), b = Buffer.from(expected || '');
  return b.length >= 24 && a.length === b.length && timingSafeEqual(a, b);
}

async function readJSON(req, limit) {
  let size = 0; const chunks = [];
  for await (const chunk of req) {
    size += chunk.length;
    if (size > limit) throw new Error('request_too_large');
    chunks.push(chunk);
  }
  const body = JSON.parse(Buffer.concat(chunks).toString() || '{}');
  if (!body || typeof body !== 'object' || Array.isArray(body)) throw new SyntaxError('object_required');
  return body;
}

function loadState(path) {
  if (!existsSync(path)) return {version: 1, invites: {}, devices: {}};
  const state = JSON.parse(readFileSync(path, 'utf8'));
  const record = value => value && typeof value === 'object' && !Array.isArray(value);
  const timestamp = value => Number.isSafeInteger(value) && value >= 0;
  const digest = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
  if (state?.version !== 1 || !state.invites || typeof state.invites !== 'object' ||
      !state.devices || typeof state.devices !== 'object' ||
      Array.isArray(state.invites) || Array.isArray(state.devices)) throw new Error('damaged_device_state');
  if (!Object.entries(state.invites).every(([key, invite]) => digest(key) && record(invite) && timestamp(invite.expires_at)) ||
      !Object.entries(state.devices).every(([id, device]) => uuid(id) && record(device) && device.id === id &&
        digest(device.token_hash) && (device.registration_owner === undefined || digest(device.registration_owner)) &&
        typeof device.name === 'string' && device.name.trim() && [...device.name].length <= 80 &&
        timestamp(device.created_at) && (device.revoked_at === null || timestamp(device.revoked_at)) &&
        typeof device.day === 'string' && (device.day === '' || /^\d{4}-\d{2}-\d{2}$/.test(device.day)) &&
        Number.isSafeInteger(device.count) && device.count >= 0 && Array.isArray(device.turn_ids) &&
        device.turn_ids.length <= 600 && device.turn_ids.every(uuid) && new Set(device.turn_ids).size === device.turn_ids.length)) {
    throw new Error('damaged_device_state');
  }
  return state;
}

function saveState(path, state) {
  mkdirSync(dirname(path), {recursive: true, mode: 0o700});
  const temp = `${path}.${randomUUID()}.tmp`;
  const file = openSync(temp, 'wx', 0o600);
  try { writeFileSync(file, JSON.stringify(state)); fsyncSync(file); }
  catch (error) { closeSync(file); try { unlinkSync(temp); } catch {} throw error; }
  closeSync(file);
  try {
    renameSync(temp, path);
    const directory = openSync(dirname(path), 'r');
    try { fsyncSync(directory); } finally { closeSync(directory); }
  }
  catch (error) { try { unlinkSync(temp); } catch {} throw error; }
}

function validMessages(value) {
  return Array.isArray(value) && value.length > 0 && value.length <= 24 &&
    value.every(message => message && typeof message === 'object' && !Array.isArray(message) &&
      Object.keys(message).sort().join(',') === 'content,role' &&
      ['user', 'assistant'].includes(message.role) && typeof message.content === 'string' &&
      message.content.trim() && [...message.content].length <= 4096) &&
    value.at(-1).role === 'user';
}

async function streamProvider({fetchImpl, key, messages, signal, emit}) {
  const response = await fetchImpl('https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions', {
    method: 'POST', redirect: 'error', signal,
    headers: {'Authorization': `Bearer ${key}`, 'Content-Type': 'application/json'},
    body: JSON.stringify({model: 'doubao-seed-2.0-mini', stream: true, max_tokens: 1024,
      thinking: {type: 'disabled'}, messages: [
        {role: 'system', content: '你是 Norma。先给结论；简单问题一句话，必要时再补一句依据或下一步，用户要细节再展开。用直接、自然的中文；只说影响判断的事实，不加开场寒暄、不复述、不报内部过程。可以讨论和建议，但不能声称 Mac 已执行任务。不要输出私有推理。'},
        ...messages]})
  });
  if (!response.ok || !response.body) throw new Error('provider_unavailable');
  const decoder = new TextDecoder('utf-8', {fatal: true});
  let buffer = '', total = 0, stopped = false, done = false;
  for await (const chunk of response.body) {
    buffer += decoder.decode(chunk, {stream: true});
    if (Buffer.byteLength(buffer) > 16384) throw new Error('provider_invalid_stream');
    let end;
    while ((end = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, end).trim(); buffer = buffer.slice(end + 1);
      if (!line || line.startsWith(':')) continue;
      if (!line.startsWith('data: ')) throw new Error('provider_invalid_stream');
      if (line === 'data: [DONE]') { done = true; break; }
      const value = JSON.parse(line.slice(6));
      const choice = value.choices?.[0];
      if (!choice) throw new Error('provider_invalid_stream');
      const delta = choice.delta?.content;
      if (delta !== undefined) {
        if (typeof delta !== 'string' || Buffer.byteLength(delta) > 16384) throw new Error('provider_invalid_stream');
        total += Buffer.byteLength(delta);
        if (total > 65536) throw new Error('provider_reply_too_large');
        if (delta) emit({type: 'delta', text: delta});
      }
      if (choice.finish_reason === 'stop') stopped = true;
      else if (choice.finish_reason != null) throw new Error('provider_incomplete');
    }
    if (done) break;
  }
  if (!done || !stopped || !total) throw new Error('provider_incomplete');
}

/// HTTP routes for invited iPhones. Voice routes retain their existing Mac-signed authorization.
export function createMobileTextService({env, fetchImpl, now}) {
  const path = env.NORMA_CLOUD_DEVICE_STATE;
  let state, broken = !path;
  if (path) {
    try { state = loadState(path); }
    catch { broken = true; }
  }
  const active = new Set();
  const configuredLimit = Number(env.NORMA_TEXT_DAILY_LIMIT || 300);
  const dailyLimit = Number.isInteger(configuredLimit) && configuredLimit > 0 && configuredLimit <= 10000
    ? configuredLimit : 300;
  const commit = next => {
    try { saveState(path, next); state = next; }
    catch (error) { broken = true; throw error; }
  };
  const authenticate = req => {
    if (broken) return {status: 503, error: 'device_state_unavailable'};
    const token = tokenFrom(req);
    const device = token && /^[A-Za-z0-9_-]{43}$/.test(token)
      ? Object.values(state.devices).find(item => item.token_hash === hash(token)) : undefined;
    if (!device) return {status: 401, error: 'unauthorized'};
    if (device.revoked_at !== null) return {status: 403, error: 'revoked'};
    return {status: 200, device};
  };

  return {authorize(req) {
    const result = authenticate(req);
    return {status: result.status, deviceID: result.device?.id};
  }, async handle(req, res, route) {
    if (!routes.has(route) || req.method !== 'POST') return false;
    if (broken) { json(res, 503, {error: 'device_state_unavailable'}); return true; }
    if (req.headers['content-type']?.split(';')[0] !== 'application/json') {
      json(res, 415, {error: 'json_required'}); return true;
    }
    const adminRoute = route.startsWith('/v1/admin/');
    if (adminRoute && !sameSecret(tokenFrom(req), env.COMPANION_ACCESS_TOKEN)) {
      json(res, 401, {error: 'unauthorized'}); return true;
    }
    let device;
    if (route === '/v1/text/turn') {
      const result = authenticate(req);
      if (result.status !== 200) { json(res, result.status, {error: result.error}); return true; }
      device = result.device;
    }
    let body;
    try { body = await readJSON(req, route === '/v1/text/turn' ? 32768 : 4096); }
    catch (error) {
      json(res, error.message === 'request_too_large' ? 413 : 400,
        {error: error.message === 'request_too_large' ? 'request_too_large' : 'invalid_request'});
      return true;
    }
    // Reading a slow body yields to other requests: recheck the authoritative state before any effect.
    if (broken) { json(res, 503, {error: 'device_state_unavailable'}); return true; }
    if (device) {
      const current = state.devices[device.id];
      if (!current || current.token_hash !== device.token_hash) {
        json(res, 401, {error: 'unauthorized'}); return true;
      }
      device = current;
      if (device.revoked_at !== null) { json(res, 403, {error: 'revoked'}); return true; }
    }
    try {
      if (route === '/v1/devices/register') {
        const proof = pairedRegistration(body, env, now());
        if (!proof) { json(res, 401, {error: 'unauthorized'}); return true; }
        const existing = Object.values(state.devices).find(device => device.registration_owner === proof.owner);
        if (existing?.revoked_at != null) { json(res, 403, {error: 'revoked'}); return true; }
        if (existing && (existing.id !== body.device_id || existing.token_hash !== proof.tokenHash) ||
            !existing && state.devices[body.device_id]) {
          json(res, 409, {error: 'registration_conflict'}); return true;
        }
        if (!existing) {
          const next = structuredClone(state);
          next.devices[body.device_id] = {id: body.device_id, name: body.name.trim(), token_hash: proof.tokenHash,
            registration_owner: proof.owner, created_at: now(), revoked_at: null, day: '', count: 0, turn_ids: []};
          commit(next);
        }
        json(res, 200, {device_id: body.device_id});
      } else if (route === '/v1/admin/devices/invite') {
        const code = randomBytes(32).toString('base64url');
        const next = structuredClone(state);
        next.invites[hash(code)] = {expires_at: now() + 7 * 86_400_000};
        commit(next);
        json(res, 200, {code, expires_at: now() + 7 * 86_400_000});
      } else if (route === '/v1/admin/devices/list') {
        json(res, 200, {devices: Object.values(state.devices).map(device => ({
          device_id: device.id, name: device.name, created_at: device.created_at,
          revoked_at: device.revoked_at, day: device.day, count: device.count}))});
      } else if (route === '/v1/admin/devices/revoke') {
        if (!uuid(body.device_id)) { json(res, 400, {error: 'invalid_request'}); return true; }
        if (!state.devices[body.device_id]) { json(res, 404, {error: 'device_not_found'}); return true; }
        const next = structuredClone(state);
        next.devices[body.device_id].revoked_at = now();
        commit(next);
        json(res, 200, {revoked: true});
      } else if (route === '/v1/devices/claim') {
        if (typeof body.code !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(body.code) ||
            typeof body.name !== 'string' || !body.name.trim() || [...body.name].length > 80) {
          json(res, 400, {error: 'invalid_request'}); return true;
        }
        const inviteHash = hash(body.code);
        if (!state.invites[inviteHash] || state.invites[inviteHash].expires_at <= now()) {
          json(res, 403, {error: 'invitation_invalid'}); return true;
        }
        const deviceID = randomUUID(), token = randomBytes(32).toString('base64url');
        const next = structuredClone(state);
        delete next.invites[inviteHash];
        next.devices[deviceID] = {id: deviceID, name: body.name.trim(), token_hash: hash(token),
          created_at: now(), revoked_at: null, day: '', count: 0, turn_ids: []};
        commit(next);
        json(res, 200, {device_id: deviceID, token});
      } else {
        if (!uuid(body.turn_id) || !validMessages(body.messages)) {
          json(res, 400, {error: 'invalid_request'}); return true;
        }
        if (device.turn_ids.some(id => id.toLowerCase() === body.turn_id.toLowerCase())) {
          json(res, 409, {error: 'duplicate_turn'}); return true;
        }
        if (active.has(device.token_hash)) {
          json(res, 429, {error: 'busy', retry_after: 1}, {'Retry-After': '1'}); return true;
        }
        const day = new Date(now()).toISOString().slice(0, 10);
        const count = device.day === day ? device.count : 0;
        if (count >= dailyLimit) {
          const seconds = Math.max(1, Math.ceil((Date.parse(`${day}T00:00:00Z`) + 86_400_000 - now()) / 1000));
          json(res, 429, {error: 'quota_exceeded', retry_after: seconds}, {'Retry-After': String(seconds)});
          return true;
        }
        if (!env.DOUBAO_SPEECH_API_KEY) { json(res, 503, {error: 'provider_unavailable'}); return true; }
        const next = structuredClone(state);
        const updated = next.devices[device.id];
        updated.day = day; updated.count = count + 1;
        updated.turn_ids = [...updated.turn_ids, body.turn_id].slice(-600);
        commit(next);
        active.add(device.token_hash);
        const abort = new AbortController();
        const deadline = AbortSignal.timeout(30000);
        res.once('close', () => abort.abort());
        res.writeHead(200, {'Content-Type': 'application/x-ndjson; charset=utf-8',
          'Cache-Control': 'no-store', 'X-Accel-Buffering': 'no'});
        const emit = event => { if (!res.destroyed) res.write(JSON.stringify(event) + '\n'); };
        emit({type: 'start', turn_id: body.turn_id});
        try {
          await streamProvider({fetchImpl, key: env.DOUBAO_SPEECH_API_KEY, messages: body.messages,
            signal: AbortSignal.any([abort.signal, deadline]), emit});
          emit({type: 'done', turn_id: body.turn_id});
        } catch { emit({type: 'error', code: deadline.aborted ? 'provider_timeout' : 'provider_unavailable'}); }
        finally { active.delete(device.token_hash); res.end(); }
      }
    } catch {
      if (!res.headersSent) json(res, 503, {error: 'device_state_unavailable'});
      else res.end();
    }
    return true;
  }};
}
