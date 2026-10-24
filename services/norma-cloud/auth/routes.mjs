import {createHash, randomBytes, timingSafeEqual} from 'node:crypto';
import {completeLogin, defaultProviders} from './providers/index.mjs';
import {loadAuthConfig} from './config.mjs';
import {createFileAccountStore} from './accounts/file-store.mjs';
import {toProfile} from './accounts/store.mjs';
import {createSessions} from './sessions.mjs';

const PENDING_TTL_MS = 10 * 60 * 1000;
const CODE_TTL_MS = 60 * 1000;
const MAX_BODY = 4096;
const SCHEMA_VERSION = 1;

const text = (res, status, body) => { res.writeHead(status, {'content-type': 'text/plain'}); res.end(body); };
const json = (res, status, body) => { res.writeHead(status, {'content-type': 'application/json', 'cache-control': 'no-store'}); res.end(JSON.stringify(body)); };
const redirect = (res, url) => { res.writeHead(302, {location: url, 'cache-control': 'no-store'}); res.end(); };

async function readJson(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY) return {tooLarge: true};
    chunks.push(chunk);
  }
  try { return {value: JSON.parse(Buffer.concat(chunks).toString() || '{}')}; } catch { return {invalid: true}; }
}

const bearer = req => (req.headers.authorization || '').replace(/^Bearer /, '');

// 返回 null 表示未启用；返回 {handle} 供 server.mjs 挂载 /v1/auth/*。
export function createAuth(env = process.env, {providers = defaultProviders, fetchImpl = fetch, now = Date.now, store} = {}) {
  const config = loadAuthConfig(env, providers);
  if (!config) return null;
  store ??= createFileAccountStore(config.accountsPath);
  const sessions = createSessions({signingKey: config.signingKey, store, now});
  const pending = new Map(); // server_state -> 授权开始记录（内存、一次性、10 分钟）
  const codes = new Map(); // code -> 一次性授权码（内存、60 秒）
  const sweep = () => {
    for (const map of [pending, codes]) for (const [key, value] of map) if (value.exp <= now()) map.delete(key);
  };

  function start(res, id, query) {
    const provider = providers[id];
    const settings = config.providers[id];
    if (!provider || !settings) return text(res, 404, 'unknown provider');
    const challenge = query.get('code_challenge') || '';
    const state = query.get('state') || '';
    const port = Number(query.get('redirect_port'));
    if (!/^[A-Za-z0-9_-]{43}$/.test(challenge)) return text(res, 400, 'invalid code_challenge');
    if (!/^[A-Za-z0-9_-]{16,128}$/.test(state)) return text(res, 400, 'invalid state');
    if (!Number.isInteger(port) || port < 1024 || port > 65535) return text(res, 400, 'invalid redirect_port');
    sweep();
    const serverState = randomBytes(32).toString('base64url');
    pending.set(serverState, {id, challenge, state, port, exp: now() + PENDING_TTL_MS});
    const url = new URL(provider.authorizeUrl);
    url.search = new URLSearchParams({client_id: settings.clientId, response_type: 'code', scope: provider.scope,
      redirect_uri: `${config.publicOrigin}/v1/auth/${id}/callback`, state: serverState}).toString();
    redirect(res, url.toString());
  }

  async function callback(res, id, query) {
    const record = pending.get(query.get('state') || '');
    if (!record || record.id !== id || record.exp <= now()) return text(res, 400, 'invalid or expired state');
    pending.delete(query.get('state')); // 单次有效
    const back = params => redirect(res, `http://127.0.0.1:${record.port}/callback?${new URLSearchParams({...params, state: record.state})}`);
    if (query.get('error')) return back({error: 'access_denied'});
    try {
      const profile = await completeLogin(providers[id], {code: query.get('code') || '',
        redirectUri: `${config.publicOrigin}/v1/auth/${id}/callback`, ...config.providers[id]}, fetchImpl);
      const account = store.upsertFromLogin({provider: id, ...profile});
      const code = randomBytes(32).toString('base64url');
      codes.set(code, {accountId: account.account_id, challenge: record.challenge, exp: now() + CODE_TTL_MS});
      back({code});
    } catch {
      back({error: 'server_error'});
    }
  }

  async function token(req, res) {
    const {value, tooLarge, invalid} = await readJson(req);
    if (tooLarge) return json(res, 413, {error: 'request_too_large'});
    if (invalid || typeof value.code !== 'string' || typeof value.code_verifier !== 'string') return json(res, 400, {error: 'invalid_request'});
    sweep();
    const record = codes.get(value.code);
    codes.delete(value.code); // 先作废再校验：错误尝试也不能重试
    if (!record || record.exp <= now() || !/^[A-Za-z0-9_-]{43,128}$/.test(value.code_verifier)) return json(res, 400, {error: 'invalid_grant'});
    const expected = Buffer.from(record.challenge);
    const actual = Buffer.from(createHash('sha256').update(value.code_verifier).digest('base64url'));
    if (expected.length !== actual.length || !timingSafeEqual(expected, actual)) return json(res, 400, {error: 'invalid_grant'});
    const account = store.get(record.accountId);
    json(res, 200, {schema_version: SCHEMA_VERSION, ...sessions.issue(record.accountId), profile: toProfile(account)});
  }

  async function refresh(req, res) {
    const {value, tooLarge, invalid} = await readJson(req);
    if (tooLarge) return json(res, 413, {error: 'request_too_large'});
    if (invalid) return json(res, 400, {error: 'invalid_request'});
    const issued = sessions.refresh(value.refresh_token);
    if (!issued) return json(res, 401, {error: 'invalid_token'});
    const auth = sessions.authenticate(issued.access_token);
    json(res, 200, {schema_version: SCHEMA_VERSION, ...issued, profile: toProfile(auth.account)});
  }

  async function handle(req, res) {
    const url = new URL(req.url || '/', 'http://localhost');
    const parts = url.pathname.split('/').filter(Boolean); // ['v1','auth', ...]
    const [, , a, b] = parts;
    try {
      if (req.method === 'GET' && b === 'start') return start(res, a, url.searchParams);
      if (req.method === 'GET' && b === 'callback') return await callback(res, a, url.searchParams);
      if (req.method === 'POST' && a === 'token' && !b) return await token(req, res);
      if (req.method === 'POST' && a === 'refresh' && !b) return await refresh(req, res);
      if (req.method === 'GET' && a === 'me' && !b) {
        const auth = sessions.authenticate(bearer(req));
        return auth ? json(res, 200, toProfile(auth.account)) : json(res, 401, {error: 'invalid_token'});
      }
      if (req.method === 'POST' && a === 'logout' && !b) {
        const auth = sessions.authenticate(bearer(req));
        if (!auth) return json(res, 401, {error: 'invalid_token'});
        sessions.revoke(auth.sid);
        res.writeHead(204); return res.end();
      }
      text(res, 404, 'not found');
    } catch {
      if (!res.headersSent) json(res, 500, {error: 'server_error'}); // 不回显内部细节
    }
  }
  return {handle};
}
