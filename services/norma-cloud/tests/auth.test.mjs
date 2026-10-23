import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {createServer} from 'node:http';
import {mkdtempSync, statSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createAuth} from '../auth/routes.mjs';
import {loadAuthConfig} from '../auth/config.mjs';
import {defaultProviders} from '../auth/providers/index.mjs';

const SIGNING = 'ab'.repeat(32);
const verifier = 'v'.repeat(43);
const challenge = createHash('sha256').update(verifier).digest('base64url');
const clientState = 'client-state-0123456789';
const baseEnv = path => ({
  NORMA_AUTH_SIGNING_KEY: SIGNING, NORMA_AUTH_PUBLIC_ORIGIN: 'https://cloud.test/norma', NORMA_AUTH_ACCOUNTS_PATH: path,
  GITHUB_CLIENT_ID: 'gh-id', GITHUB_CLIENT_SECRET: 'gh-secret-value', GOOGLE_CLIENT_ID: 'gg-id', GOOGLE_CLIENT_SECRET: 'gg-secret-value',
});
const json = body => ({ok: true, json: async () => body});
// 假提供方：按 URL 返回固定响应，同邮箱同时给 GitHub 与 Google 以验证“邮箱不参与身份”。
function fakeFetch({githubId = 42, googleSub = 'g-1', fail = false} = {}) {
  return async url => {
    if (fail) return {ok: false, json: async () => ({})};
    if (url.includes('github.com/login/oauth/access_token')) return json({access_token: 'gho_x'});
    if (url.endsWith('api.github.com/user')) return json({id: githubId, login: 'zk', name: 'Zhou Kai', avatar_url: 'https://a/gh.png', email: 'zk@example.com'});
    if (url.includes('oauth2.googleapis.com/token')) return json({access_token: 'ya29'});
    if (url.includes('openidconnect.googleapis.com')) return json({sub: googleSub, name: '周凯', email: 'zk@example.com', picture: 'https://a/gg.png'});
    throw new Error(`unexpected fetch ${url}`);
  };
}
async function setup(t, options = {}, env) {
  const dir = mkdtempSync(join(tmpdir(), 'auth-'));
  const path = join(dir, 'accounts.json');
  const clock = {value: 1_800_000_000_000};
  const make = () => createAuth(env ?? baseEnv(path), {fetchImpl: fakeFetch(options.fetch), now: () => clock.value, ...options.auth});
  const auth = make();
  const server = createServer((req, res) => auth.handle(req, res));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  return {origin: `http://127.0.0.1:${server.address().port}`, clock, path, make};
}
const get = (url, headers = {}) => fetch(url, {redirect: 'manual', headers});
const post = (url, body, headers = {}) => fetch(url, {method: 'POST', headers: {'content-type': 'application/json', ...headers},
  body: typeof body === 'string' ? body : JSON.stringify(body)});
async function login(ctx, provider = 'github', {port = 53682, query = ''} = {}) {
  const started = await get(`${ctx.origin}/v1/auth/${provider}/start?code_challenge=${challenge}&state=${clientState}&redirect_port=${port}`);
  const authorize = new URL(started.headers.get('location'));
  const serverState = authorize.searchParams.get('state');
  const back = await get(`${ctx.origin}/v1/auth/${provider}/callback?state=${serverState}&code=prov-code${query}`);
  return {started, authorize, serverState, back, backUrl: new URL(back.headers.get('location'))};
}
async function exchange(ctx, code, codeVerifier = verifier) {
  return post(`${ctx.origin}/v1/auth/token`, {code, code_verifier: codeVerifier});
}

/// 完整授权：start → 提供方 → callback → 回环 → token → me。
/// 数据构造：GitHub，端口 53682，challenge=S256(43 个 'v')。
/// 执行过程：逐步跟随 302，最后用 code+verifier 换会话，再用访问令牌读取 profile。
/// 预期结果：授权地址带服务端 client_id/scope/redirect_uri；回环带 code 与原 client state；会话 profile 含 1 个 github 身份。
test('full authorization code flow with PKCE', async t => {
  const ctx = await setup(t);
  const {started, authorize, back, backUrl} = await login(ctx);
  assert.equal(started.status, 302, 'start 应重定向到提供方');
  assert.equal(authorize.origin + authorize.pathname, 'https://github.com/login/oauth/authorize', '应指向 GitHub 授权页');
  assert.equal(authorize.searchParams.get('client_id'), 'gh-id', '应使用服务端 client_id');
  assert.equal(authorize.searchParams.get('scope'), 'read:user user:email', 'scope 仅限读取资料');
  assert.equal(authorize.searchParams.get('redirect_uri'), 'https://cloud.test/norma/v1/auth/github/callback', '回调指向服务端');
  assert.equal(back.status, 302, 'callback 应重定向回回环地址');
  assert.equal(backUrl.origin, 'http://127.0.0.1:53682', '只能回到回环地址与请求的端口');
  assert.equal(backUrl.searchParams.get('state'), clientState, '应回传客户端原 state');
  const res = await exchange(ctx, backUrl.searchParams.get('code'));
  const session = await res.json();
  assert.equal(res.status, 200, 'token 交换应成功');
  assert.equal(session.schema_version, 1, '会话应带 schema_version');
  assert.deepEqual(session.profile.identities, [{provider: 'github', subject: '42'}], '身份由 provider+subject 决定');
  assert.equal(JSON.stringify(session).includes('gh-secret-value'), false, '响应不得含 client secret');
  const me = await get(`${ctx.origin}/v1/auth/me`, {authorization: `Bearer ${session.access_token}`});
  assert.equal((await me.json()).account_id, session.profile.account_id, 'me 应返回同一账号');
});

/// 授权码单次有效、60 秒过期、错误 verifier 被拒且作废。
/// 数值推导：码 TTL = 60s；时钟前进 61_000ms 即过期。
/// 预期结果：重放 → 400；过期 → 400；错误 verifier → 400 且随后正确 verifier 也失败（已作废）。
test('authorization code is single use, short lived and PKCE bound', async t => {
  const ctx = await setup(t);
  const code = (await login(ctx)).backUrl.searchParams.get('code');
  assert.equal((await exchange(ctx, code)).status, 200, '首次使用应成功');
  assert.equal((await exchange(ctx, code)).status, 400, '重放同一 code 必须拒绝');

  const expired = (await login(ctx)).backUrl.searchParams.get('code');
  ctx.clock.value += 61_000;
  assert.equal((await exchange(ctx, expired)).status, 400, '超过 60 秒的 code 必须拒绝');

  const wrong = (await login(ctx)).backUrl.searchParams.get('code');
  assert.equal((await exchange(ctx, wrong, 'w'.repeat(43))).status, 400, '错误 verifier 必须拒绝');
  assert.equal((await exchange(ctx, wrong)).status, 400, '失败尝试后 code 已作废，不能再试');
});

/// start 的参数校验与开放重定向防护。
/// 预期结果：端口 80/70000/abc、坏 challenge、短 state → 400；未知或未启用提供方 → 404；端口之外无法指定跳转目标。
test('start validates parameters and only redirects to loopback ports', async t => {
  const ctx = await setup(t);
  const start = (params, provider = 'github') => get(`${ctx.origin}/v1/auth/${provider}/start?${new URLSearchParams(params)}`);
  const ok = {code_challenge: challenge, state: clientState, redirect_port: '53682'};
  assert.equal((await start(ok)).status, 302, '合法参数应通过');
  for (const port of ['80', '70000', 'abc', '']) assert.equal((await start({...ok, redirect_port: port})).status, 400, `端口 ${port} 必须拒绝`);
  assert.equal((await start({...ok, code_challenge: 'short'})).status, 400, '非法 challenge 必须拒绝');
  assert.equal((await start({...ok, state: 'short'})).status, 400, '过短 state 必须拒绝');
  assert.equal((await start(ok, 'apple')).status, 404, '未注册提供方必须 404');
  assert.equal((await start({...ok, redirect_uri: 'https://evil.test'})).headers.get('location').includes('evil.test'), false, '不接受任意跳转目标');
});

/// 提供方拒绝与 state 重放。
/// 预期结果：error 回调 → 回环 error=access_denied 且无 code；同一 state 再回调 → 400；过期 state（>10 分钟）→ 400。
test('provider denial, pending state replay and expiry', async t => {
  const ctx = await setup(t);
  const denied = await login(ctx, 'github', {query: '&error=access_denied'});
  assert.equal(denied.backUrl.searchParams.get('error'), 'access_denied', '应回传拒绝原因');
  assert.equal(denied.backUrl.searchParams.has('code'), false, '拒绝时不得签发 code');
  assert.equal((await get(`${ctx.origin}/v1/auth/github/callback?state=${denied.serverState}&code=x`)).status, 400, 'state 只能用一次');

  const started = await get(`${ctx.origin}/v1/auth/github/start?code_challenge=${challenge}&state=${clientState}&redirect_port=53682`);
  const stale = new URL(started.headers.get('location')).searchParams.get('state');
  ctx.clock.value += 10 * 60 * 1000 + 1;
  assert.equal((await get(`${ctx.origin}/v1/auth/github/callback?state=${stale}&code=x`)).status, 400, '超过 10 分钟的 state 必须拒绝');
});

/// 提供方失败不泄露细节。
/// 预期结果：回环 error=server_error；回环地址与响应体不含任何 secret。
test('provider failure returns generic error without secrets', async t => {
  const ctx = await setup(t, {fetch: {fail: true}});
  const {backUrl, back} = await login(ctx);
  assert.equal(backUrl.searchParams.get('error'), 'server_error', '提供方失败应返回通用错误');
  assert.equal(backUrl.toString().includes('secret'), false, '回跳地址不得含 secret');
  assert.equal((await back.text()).includes('secret'), false, '响应体不得含 secret');
});

/// 账号身份：同一身份同一账号；同邮箱不同提供方不同账号。
/// 预期结果：GitHub 登录两次 account_id 相同；Google 与 GitHub 邮箱相同但 account_id 不同，各自只有 1 个身份。
test('identity is provider plus subject, never email', async t => {
  const ctx = await setup(t);
  const sessionOf = async provider => (await (await exchange(ctx, (await login(ctx, provider)).backUrl.searchParams.get('code'))).json());
  const [gh1, gh2, gg] = [await sessionOf('github'), await sessionOf('github'), await sessionOf('google')];
  assert.equal(gh1.profile.account_id, gh2.profile.account_id, '同一 (provider, sub) 必须得到同一账号');
  assert.notEqual(gh1.profile.account_id, gg.profile.account_id, '同邮箱不同提供方不得自动合并');
  assert.equal(gh1.profile.email, gg.profile.email, '测试前提：两个提供方邮箱相同');
  assert.equal(gg.profile.identities.length, 1, 'Google 账号只有自己的一个身份');
});

/// 刷新轮换、复用检测、退出登录、令牌篡改。
/// 预期结果：刷新得到新令牌；旧刷新令牌被拒且整条会话作废（新访问令牌 me 也 401）；退出后 me 与刷新均 401；篡改访问令牌 401。
test('refresh rotation, reuse detection, logout and tampering', async t => {
  const ctx = await setup(t);
  const first = await (await exchange(ctx, (await login(ctx)).backUrl.searchParams.get('code'))).json();
  const refreshed = await (await post(`${ctx.origin}/v1/auth/refresh`, {refresh_token: first.refresh_token})).json();
  assert.notEqual(refreshed.refresh_token, first.refresh_token, '刷新必须轮换刷新令牌');
  assert.equal((await post(`${ctx.origin}/v1/auth/refresh`, {refresh_token: first.refresh_token})).status, 401, '旧刷新令牌必须失效');
  const me = token => get(`${ctx.origin}/v1/auth/me`, {authorization: `Bearer ${token}`});
  assert.equal((await me(refreshed.access_token)).status, 401, '检测到复用后整条会话应作废');

  const second = await (await exchange(ctx, (await login(ctx)).backUrl.searchParams.get('code'))).json();
  assert.equal((await me(second.access_token + 'x')).status, 401, '篡改签名必须拒绝');
  assert.equal((await post(`${ctx.origin}/v1/auth/logout`, {}, {authorization: `Bearer ${second.access_token}`})).status, 204, '退出登录应成功');
  assert.equal((await me(second.access_token)).status, 401, '退出后访问令牌失效');
  assert.equal((await post(`${ctx.origin}/v1/auth/refresh`, {refresh_token: second.refresh_token})).status, 401, '退出后刷新令牌失效');
});

/// 请求体上限与账号文件持久化。
/// 数值推导：上限 4096 字节，5000 字节超限。
/// 预期结果：超限 413；账号文件权限 0600；重建实例（模拟重启）后原刷新令牌仍可用。
test('body limit and persistence across restart', async t => {
  const ctx = await setup(t);
  assert.equal((await post(`${ctx.origin}/v1/auth/token`, 'x'.repeat(5000))).status, 413, '超过 4096 字节必须拒绝');
  const session = await (await exchange(ctx, (await login(ctx)).backUrl.searchParams.get('code'))).json();
  assert.equal(statSync(ctx.path).mode & 0o777, 0o600, '账号文件权限必须为 0600');
  const restarted = ctx.make();
  const server = createServer((req, res) => restarted.handle(req, res));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => { server.closeAllConnections(); server.close(); });
  const res = await post(`http://127.0.0.1:${server.address().port}/v1/auth/refresh`, {refresh_token: session.refresh_token});
  assert.equal(res.status, 200, '重启后会话应仍然有效');
});

/// 配置：未配置关闭；部分或非法配置拒绝启动且错误不含 secret；新增提供方无需改路由。
/// 预期结果：空环境 → null；缺 secret / 无提供方 / 弱签名密钥 / 非 https 来源 → 抛错；自定义提供方只靠注册表即可登录。
test('configuration gating and registry-driven providers', async t => {
  assert.equal(loadAuthConfig({}, defaultProviders), null, '没有任何相关变量时应整体关闭');
  const env = baseEnv('/tmp/x.json');
  assert.throws(() => loadAuthConfig({...env, GITHUB_CLIENT_SECRET: undefined}, defaultProviders), /GITHUB_CLIENT_SECRET/, '只配 id 不配 secret 必须拒绝');
  assert.throws(() => loadAuthConfig({NORMA_AUTH_SIGNING_KEY: SIGNING, NORMA_AUTH_PUBLIC_ORIGIN: 'https://x.test'}, defaultProviders), /at least one/, '没有任何提供方必须拒绝');
  assert.throws(() => loadAuthConfig({...env, NORMA_AUTH_SIGNING_KEY: 'abc'}, defaultProviders), /SIGNING_KEY/, '弱签名密钥必须拒绝');
  assert.throws(() => loadAuthConfig({...env, NORMA_AUTH_PUBLIC_ORIGIN: 'http://evil.test'}, defaultProviders), /https/, '非 https 来源必须拒绝');
  try { loadAuthConfig({...env, NORMA_AUTH_SIGNING_KEY: 'abc'}, defaultProviders); }
  catch (error) { assert.equal(error.message.includes('gh-secret-value'), false, '错误信息不得含 secret'); }

  const fake = {id: 'fake', authorizeUrl: 'https://fake.test/authorize', scope: 'profile',
    tokenRequest: () => ({url: 'https://fake.test/token', init: {method: 'POST'}}),
    fetchProfile: async () => ({subject: 'f-1', name: 'Fake', email: null, avatar: null})};
  const dir = mkdtempSync(join(tmpdir(), 'auth-'));
  const ctx = await setup(t, {auth: {providers: {fake}, fetchImpl: async () => json({access_token: 't'})}},
    {NORMA_AUTH_SIGNING_KEY: SIGNING, NORMA_AUTH_PUBLIC_ORIGIN: 'https://cloud.test', NORMA_AUTH_ACCOUNTS_PATH: join(dir, 'a.json'),
      FAKE_CLIENT_ID: 'f', FAKE_CLIENT_SECRET: 's'});
  const code = (await login(ctx, 'fake')).backUrl.searchParams.get('code');
  const session = await (await exchange(ctx, code)).json();
  assert.deepEqual(session.profile.identities, [{provider: 'fake', subject: 'f-1'}], '新增提供方只需注册，路由无需修改');
});
