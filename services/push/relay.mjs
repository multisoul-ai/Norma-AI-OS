import { createHash, createPrivateKey, sign, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';
import * as http2 from 'node:http2';
import { readFileSync, statSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { validateActivity, activityPayload } from './activity.mjs';

const digest = value => createHash('sha256').update(value).digest();
const result = (status, error) => ({ status, body: { error } });
const unknown = () => result(502, 'delivery_unknown');
const identifier = (value, limit = 128) => typeof value === 'string' && value.length <= limit && /^[A-Za-z0-9_.:-]+$/.test(value);
const fields = ['event_id', 'token', 'expires_at', 'kind', 'machine_id', 'workspace_id', 'route_id', 'node_id', 'run_id', 'session_id', 'turn_id', 'request_id', 'preview', 'desktop_workspace_id', 'area_id'];

function validate(input, now) {
  if (input?.kind === 'activity') return validateActivity(input, now);
  if (!input || Array.isArray(input) || typeof input !== 'object'
      || Object.keys(input).some(key => !fields.includes(key))
      || typeof input.event_id !== 'string' || !/^[a-f0-9]{8}(-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(input.event_id)
      || typeof input.token !== 'string' || !/^(?:[a-f0-9]{2}){16,256}$/.test(input.token)
      || !Number.isSafeInteger(input.expires_at) || input.expires_at <= now || input.expires_at > now + 300
      || !['approval', 'completed', 'failed', 'review'].includes(input.kind)
      || typeof input.machine_id !== 'string' || !/^mac-[a-f0-9]{64}$/.test(input.machine_id)
      || !['workspace_id', 'route_id', 'run_id', 'session_id', 'turn_id'].every(key => identifier(input[key]))
      || typeof input.node_id !== 'string' || !/^[1-9][0-9]{0,19}$/.test(input.node_id)
      || BigInt(input.node_id) > 18446744073709551615n
      || (input.kind === 'approval' ? !identifier(input.request_id, 256) : input.request_id !== undefined)) {
    throw result(400, 'invalid_event');
  }
  if ((input.desktop_workspace_id !== undefined || input.area_id !== undefined)
      && (![input.desktop_workspace_id,input.area_id].every(v=>identifier(v))
          || input.workspace_id !== `desktop:${input.desktop_workspace_id}:${input.area_id}`)) throw result(400,'invalid_event');
  if (input.preview !== undefined) {
    const p = input.preview;
    if (!p || Array.isArray(p) || typeof p !== 'object'
        || Object.keys(p).some(key => !['project', 'card', 'summary'].includes(key))
        || !Object.entries({ project: 160, card: 240, summary: 640 }).every(([key, limit]) =>
          typeof p[key] === 'string' && p[key].trim().length > 0 && Buffer.byteLength(p[key]) <= limit
          && !/[\u0000-\u001f\u007f-\u009f]/u.test(p[key]))) throw result(400, 'invalid_event');
    input = { ...input, preview: { project: p.project, card: p.card, summary: p.summary } };
  }
  // Stable property order means JSON formatting cannot bypass conflict checks.
  return Object.fromEntries(fields.filter(key => input[key] !== undefined).map(key => [key, input[key]]));
}

function body(request) {
  return new Promise((resolve, reject) => {
    let bytes = 0;
    const chunks = [];
    const finish = (error, value) => {
      clearTimeout(timer);
      request.off('data', data); request.off('end', end); request.off('error', failed); request.off('aborted', failed);
      if (error) { request.pause(); reject(error); } else { resolve(value); }
    };
    const data = chunk => {
      bytes += chunk.length;
      if (bytes > 4096) { finish(result(413, 'body_too_large')); return; }
      chunks.push(chunk);
    };
    const end = () => {
      try { finish(null, JSON.parse(Buffer.concat(chunks).toString('utf8'))); }
      catch { finish(result(400, 'invalid_json')); }
    };
    const failed = () => finish(result(400, 'incomplete_request'));
    // Five seconds bounds slow authenticated senders independently of Node's HTTP sweep timer.
    const timer = setTimeout(() => finish(result(408, 'request_timeout')), 5000);
    request.on('data', data); request.once('end', end); request.once('error', failed); request.once('aborted', failed);
  });
}

/** Private relay: the credential is provisioned to trusted Macs, never to the iOS app. */
export function createRelay({ key, deliver, now = Date.now }) {
  if (typeof key !== 'string' || !/^[a-f0-9]{64}$/.test(key)) throw new Error('Relay key must contain 32 random bytes in lowercase hex');
  const expected = digest(`Bearer ${key}`);
  const events = new Map();
  let active = 0;
  const server = createServer({ maxHeaderSize: 8192, headersTimeout: 5000, requestTimeout: 5000 }, async (request, response) => {
    const reply = outcome => {
      if (response.destroyed) return;
      response.writeHead(outcome.status, { 'content-type': 'application/json', 'cache-control': 'no-store', connection: 'close' });
      response.end(JSON.stringify(outcome.body));
      request.resume();
    };
    if (request.method === 'GET' && request.url === '/healthz') { reply({ status: 200, body: { status: 'ready' } }); return; }
    if (request.method !== 'POST' || request.url !== '/v1/notifications') { reply(result(404, 'not_found')); return; }
    const authorizationCount = request.rawHeaders.filter((_, i) => i % 2 === 0 && request.rawHeaders[i].toLowerCase() === 'authorization').length;
    if (authorizationCount !== 1 || !timingSafeEqual(expected, digest(request.headers.authorization ?? ''))) { reply(result(401, 'unauthorized')); return; }
    if (request.headers['content-type'] !== 'application/json' || request.headers['content-encoding']) { reply(result(415, 'json_required')); return; }
    if (Number(request.headers['content-length']) > 4096) { reply(result(413, 'body_too_large')); return; }
    if (active >= 8) { reply(result(503, 'relay_busy')); return; }
    active++;
    try {
      const event = validate(await body(request), Math.floor(now() / 1000));
      for (const [id, saved] of events) if (saved.expires <= now() / 1000) events.delete(id);
      const id = digest(`${event.token}:${event.event_id}`).toString('hex');
      const fingerprint = digest(JSON.stringify(event)).toString('hex');
      let saved = events.get(id);
      if (saved && saved.fingerprint !== fingerprint) { reply(result(409, 'event_conflict')); return; }
      if (!saved) {
        // ponytail: one process, 4096 live dedupe entries until event expiry (max five minutes).
        // Durable shared idempotency is required before running multiple replicas or retrying after restarts.
        if (events.size >= 4096) { reply(result(503, 'relay_busy')); return; }
        saved = { fingerprint, expires: event.expires_at, outcome: Promise.resolve().then(() => deliver(event)).catch(unknown) };
        events.set(id, saved);
      }
      reply(await saved.outcome);
    } catch (error) { reply(error?.status ? error : result(400, 'invalid_request')); }
    finally { active--; }
  });
  server.maxConnections = 64;
  return server;
}

function payload(event) {
  if (event.kind === 'activity') return activityPayload(event);
  const { token, expires_at, preview, ...target } = event;
  const messages = { approval: '有一项操作需要你确认', completed: 'Agent 的任务已完成', failed: 'Agent 的任务遇到问题', review: '有一份成果可验收' };
  const states = { approval: '待确认', completed: '已完成', failed: '遇到问题', review: '待验收' };
  const alert = preview ? { title: `${preview.card} · ${states[event.kind]}`, subtitle: preview.project, body: preview.summary }
    : { title: 'Norma', body: messages[event.kind] };
  return JSON.stringify({ aps: { alert, sound: 'default' }, norma: { version: 1, ...target } });
}

export function createAPNs({ privateKey, teamID, keyID, environment, topic }, { connect = http2.connect, now = Date.now } = {}) {
  if (!/^[A-Z0-9]{10}$/.test(teamID) || !/^[A-Z0-9]{10}$/.test(keyID)
      || !['sandbox', 'production'].includes(environment) || topic !== 'com.norma-ai.os.ios'
      || privateKey.type !== 'private' || privateKey.asymmetricKeyType !== 'ec' || privateKey.asymmetricKeyDetails?.namedCurve !== 'prime256v1') {
    throw new Error('APNs requires a P-256 key, Team ID, Key ID, explicit environment and Norma topic');
  }
  const origin = environment === 'sandbox' ? 'https://api.sandbox.push.apple.com' : 'https://api.push.apple.com';
  let current, jwt, issuedAt, closed = false;
  const sessions = new Set();
  function authorization() {
    const seconds = Math.floor(now() / 1000);
    // Apple's allowed refresh window is 20–60 minutes; 50 leaves ten minutes for clock/scheduling drift.
    if (!jwt || seconds - issuedAt >= 50 * 60 || seconds < issuedAt) {
      issuedAt = seconds;
      const header = Buffer.from(JSON.stringify({ alg: 'ES256', kid: keyID })).toString('base64url');
      const claims = Buffer.from(JSON.stringify({ iss: teamID, iat: seconds })).toString('base64url');
      const content = `${header}.${claims}`;
      jwt = content + '.' + sign('sha256', Buffer.from(content), { key: privateKey, dsaEncoding: 'ieee-p1363' }).toString('base64url');
    }
    return `bearer ${jwt}`;
  }
  function session() {
    if (!current || current.closed || current.destroyed) {
      const fresh = connect(origin);
      sessions.add(fresh); current = fresh;
      fresh.on('error', () => {}); // Each affected stream reports delivery_unknown; never expose tokens in logs.
      fresh.once('close', () => sessions.delete(fresh));
      fresh.on('goaway', () => { if (current === fresh) current = undefined; fresh.close(); });
      fresh.setTimeout(60_000, () => fresh.destroy());
    }
    return current;
  }
  async function send(event) {
    if (closed) return result(503, 'relay_stopped');
    if (event.expires_at <= now() / 1000) return result(400, 'event_expired');
    try {
      const connection = session();
      return await new Promise(resolve => {
        let finished = false, status = 0, bytes = 0;
        const chunks = [];
        const stream = connection.request({
          ':method': 'POST', ':path': `/3/device/${event.token}`, authorization: authorization(),
          'apns-topic': event.kind === 'activity' ? `${topic}.push-type.liveactivity` : topic,
          'apns-push-type': event.kind === 'activity' ? 'liveactivity' : 'alert', 'apns-id': event.event_id,
          'apns-expiration': String(event.expires_at), 'apns-priority': event.kind === 'activity' ? String(event.priority) : event.kind === 'approval' ? '10' : '5',
          [http2.sensitiveHeaders]: ['authorization', ':path'],
        });
        const finish = outcome => { if (!finished) { finished = true; clearTimeout(timer); resolve(outcome); } };
        // A request is never retried when APNs may already have received it.
        const timer = setTimeout(() => { finish(unknown()); stream.close(http2.constants.NGHTTP2_CANCEL); }, 10_000);
        stream.on('response', headers => { status = Number(headers[':status']); });
        stream.on('data', chunk => {
          bytes += chunk.length;
          if (bytes > 4096) { finish(unknown()); stream.close(http2.constants.NGHTTP2_CANCEL); }
          else { chunks.push(chunk); }
        });
        stream.once('error', () => finish(unknown()));
        stream.once('close', () => finish(unknown()));
        stream.once('end', () => {
          if (status === 200) { finish({ status: 200, body: { status: 'accepted' } }); return; }
          let reason;
          try { reason = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { finish(unknown()); return; }
          if (status === 410 && reason.reason === 'Unregistered' && Number.isSafeInteger(reason.timestamp) && reason.timestamp >= 0) {
            finish({ status: 410, body: { error: 'token_unregistered', invalidated_at: reason.timestamp } });
          } else if (status === 400 && ['BadDeviceToken', 'DeviceTokenNotForTopic'].includes(reason.reason)) {
            finish(result(422, 'token_invalid'));
          } else if (status === 403) { finish(result(502, 'apns_configuration_error')); }
          else if (status === 429 || status >= 500) { finish(result(503, 'apns_unavailable')); }
          else { finish(result(502, 'apns_rejected')); }
        });
        stream.end(payload(event));
      });
    } catch { return unknown(); }
  }
  // ponytail: one APNs stream at a time, at most eight accepted requests in the relay.
  // Multiplex only after the first token-authenticated success if measured bursts require it.
  let queue = Promise.resolve();
  return { send: event => {
    const operation = queue.then(() => send(event));
    queue = operation.catch(() => {});
    return operation;
  }, close: () => { closed = true; for (const connection of sessions) connection.destroy(); } };
}

function secretFile(path) {
  if (!path || !statSync(path).isFile() || statSync(path).size > 16 * 1024 || (statSync(path).mode & 0o077) !== 0) {
    throw new Error('Secret files must exist, be at most 16 KiB and have owner-only permissions (chmod 600)');
  }
  return readFileSync(path, 'utf8').trim();
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const env = process.env;
    const port = Number(env.NORMA_PUSH_PORT ?? '8788');
    if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid relay port');
    const apns = createAPNs({ privateKey: createPrivateKey(secretFile(env.APNS_KEY_PATH)), teamID: env.APNS_TEAM_ID,
      keyID: env.APNS_KEY_ID, environment: env.APNS_ENVIRONMENT, topic: 'com.norma-ai.os.ios' });
    const relay = createRelay({ key: secretFile(env.NORMA_PUSH_RELAY_KEY_FILE), deliver: apns.send });
    relay.on('error', () => { apns.close(); console.error('Push relay could not listen'); process.exitCode = 1; });
    // TLS terminates at the deployment proxy; never expose this HTTP listener on a public interface.
    relay.listen(port, '127.0.0.1', () => console.log(`Norma push relay listening on 127.0.0.1:${port}`));
    const close = () => { relay.close(); relay.closeAllConnections(); apns.close(); };
    process.once('SIGINT', close); process.once('SIGTERM', close);
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
