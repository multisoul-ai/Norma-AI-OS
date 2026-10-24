// 本地开发/验收用：带假身份提供方的账号登录服务，无需真实 OAuth 凭据。
// 用法：node scripts/dev-auth-server.mjs [端口，默认 18791]
// App 侧：NORMA_CLOUD_URL=http://127.0.0.1:18791 NORMA_ACCOUNT_LOGIN=1 cargo run -p app
// 假提供方的授权页会立即同意并回跳，只用于本机；绝不要部署。
import {createServer} from 'node:http';
import {mkdtempSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createAuth} from '../auth/routes.mjs';
import {defaultProviders} from '../auth/providers/index.mjs';

const port = Number(process.argv[2] || 18791);
const origin = `http://127.0.0.1:${port}`;
const profiles = {github: {subject: '1001', name: 'Zhou Kai', email: 'zhoukai@example.com'},
  google: {subject: 'g-1001', name: '周凯', email: 'zhoukai@example.com'}};

const providers = Object.fromEntries(Object.entries(defaultProviders).map(([id, provider]) => [id, {
  ...provider,
  authorizeUrl: `${origin}/fake-idp/${id}/authorize`,
  tokenRequest: () => ({url: `${origin}/fake-idp/token`, init: {method: 'POST'}}),
  fetchProfile: async () => ({...profiles[id], avatar: null}),
}]));
const fetchImpl = async () => ({ok: true, json: async () => ({access_token: 'fake'})});

const env = {NORMA_AUTH_SIGNING_KEY: 'cd'.repeat(32), NORMA_AUTH_PUBLIC_ORIGIN: origin,
  NORMA_AUTH_ACCOUNTS_PATH: join(mkdtempSync(join(tmpdir(), 'norma-dev-auth-')), 'accounts.json'),
  GITHUB_CLIENT_ID: 'dev', GITHUB_CLIENT_SECRET: 'dev', GOOGLE_CLIENT_ID: 'dev', GOOGLE_CLIENT_SECRET: 'dev'};
const auth = createAuth(env, {providers, fetchImpl});

createServer((req, res) => {
  const url = new URL(req.url, origin);
  if (url.pathname.startsWith('/v1/auth/')) return auth.handle(req, res);
  if (/^\/fake-idp\/[a-z]+\/authorize$/.test(url.pathname)) {
    const back = new URL(url.searchParams.get('redirect_uri'));
    back.search = new URLSearchParams({code: 'fake-code', state: url.searchParams.get('state')}).toString();
    res.writeHead(302, {location: back.toString()});
    return res.end();
  }
  res.writeHead(404); res.end();
}).listen(port, '127.0.0.1', () => console.log(`dev auth server on ${origin}`));
