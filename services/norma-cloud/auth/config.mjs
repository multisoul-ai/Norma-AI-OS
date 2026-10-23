// 账号登录配置。未设置任何相关变量则整体关闭；一旦设置任一变量，缺失或非法配置必须阻止启动。
export function loadAuthConfig(env, providers) {
  const names = Object.keys(providers);
  const envNames = ['NORMA_AUTH_SIGNING_KEY', 'NORMA_AUTH_PUBLIC_ORIGIN', 'NORMA_AUTH_ACCOUNTS_PATH',
    ...names.flatMap(id => [`${id.toUpperCase()}_CLIENT_ID`, `${id.toUpperCase()}_CLIENT_SECRET`])];
  if (!envNames.some(name => env[name] !== undefined)) return null;

  const signingKey = env.NORMA_AUTH_SIGNING_KEY;
  if (!/^[0-9a-f]{64,}$/i.test(signingKey || '')) throw new Error('NORMA_AUTH_SIGNING_KEY must be at least 64 hex characters');
  const origin = (env.NORMA_AUTH_PUBLIC_ORIGIN || '').replace(/\/+$/, '');
  if (!/^https:\/\/|^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?(\/|$)/.test(origin)) {
    throw new Error('NORMA_AUTH_PUBLIC_ORIGIN must be https (or loopback http)');
  }

  const enabled = {};
  for (const id of names) {
    const prefix = id.toUpperCase();
    const clientId = env[`${prefix}_CLIENT_ID`];
    const clientSecret = env[`${prefix}_CLIENT_SECRET`];
    if (clientId === undefined && clientSecret === undefined) continue;
    if (!clientId || !clientSecret) throw new Error(`${id} requires both ${prefix}_CLIENT_ID and ${prefix}_CLIENT_SECRET`);
    enabled[id] = {clientId, clientSecret};
  }
  if (Object.keys(enabled).length === 0) throw new Error('at least one auth provider must be configured');
  return {
    signingKey: Buffer.from(signingKey, 'hex'),
    publicOrigin: origin,
    accountsPath: env.NORMA_AUTH_ACCOUNTS_PATH || '/var/lib/norma-cloud/accounts.json',
    providers: enabled,
  };
}
