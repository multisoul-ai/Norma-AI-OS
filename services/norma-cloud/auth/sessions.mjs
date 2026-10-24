import {createHash, createHmac, randomBytes, timingSafeEqual} from 'node:crypto';

export const ACCESS_TTL_SECONDS = 3600;
export const REFRESH_TTL_SECONDS = 30 * 24 * 3600;

const sha256 = value => createHash('sha256').update(value).digest('hex');
const equal = (a, b) => a.length === b.length && timingSafeEqual(Buffer.from(a), Buffer.from(b));

// 访问令牌：HMAC 签名的短期令牌，另校验会话仍存在（退出登录/刷新复用即失效）。
// 刷新令牌：`<sid>.<secret>`，服务端只存 secret 的哈希，每次刷新轮换。
export function createSessions({signingKey, store, now = Date.now}) {
  const sign = body => createHmac('sha256', signingKey).update(body).digest('base64url');
  const mint = (sid, accountId) => {
    const exp = Math.floor(now() / 1000) + ACCESS_TTL_SECONDS;
    const body = Buffer.from(JSON.stringify({sid, aid: accountId, exp})).toString('base64url');
    const secret = randomBytes(32).toString('base64url');
    store.putSession({sid, accountId, refreshHash: sha256(secret), expiresAt: now() + REFRESH_TTL_SECONDS * 1000});
    return {access_token: `${body}.${sign(body)}`, refresh_token: `${sid}.${secret}`, expires_in: ACCESS_TTL_SECONDS};
  };
  return {
    issue: accountId => mint(randomBytes(16).toString('hex'), accountId),
    authenticate(bearer) {
      const [body, signature, extra] = String(bearer || '').split('.');
      if (!body || !signature || extra !== undefined || !equal(sign(body), signature)) return null;
      let payload;
      try { payload = JSON.parse(Buffer.from(body, 'base64url').toString()); } catch { return null; }
      if (!(payload.exp > Math.floor(now() / 1000))) return null;
      const session = store.getSession(payload.sid);
      const account = session && store.get(session.accountId);
      return account ? {account, sid: payload.sid} : null;
    },
    refresh(refreshToken) {
      const [sid, secret, extra] = String(refreshToken || '').split('.');
      const session = sid && store.getSession(sid);
      if (!session || !secret || extra !== undefined || session.expiresAt <= now()) return null;
      if (!equal(session.refreshHash, sha256(secret))) {
        store.deleteSession(sid); // 复用已轮换的旧令牌：视为泄露，整条会话作废
        return null;
      }
      return mint(sid, session.accountId);
    },
    revoke: sid => store.deleteSession(sid),
  };
}
