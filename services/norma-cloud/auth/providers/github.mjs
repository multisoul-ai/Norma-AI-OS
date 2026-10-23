// GitHub OAuth 提供方。仅读取头像、昵称、邮箱（read:user user:email）。
export const github = {
  id: 'github',
  authorizeUrl: 'https://github.com/login/oauth/authorize',
  scope: 'read:user user:email',
  tokenRequest({code, redirectUri, clientId, clientSecret}) {
    return {
      url: 'https://github.com/login/oauth/access_token',
      init: {method: 'POST', headers: {accept: 'application/json', 'content-type': 'application/json'},
        body: JSON.stringify({client_id: clientId, client_secret: clientSecret, code, redirect_uri: redirectUri})},
    };
  },
  async fetchProfile(fetchImpl, accessToken) {
    const headers = {authorization: `Bearer ${accessToken}`, accept: 'application/vnd.github+json', 'user-agent': 'norma-cloud'};
    const user = await (await fetchImpl('https://api.github.com/user', {headers})).json();
    if (user?.id === undefined) throw new Error('github profile missing id');
    let email = user.email || null;
    if (!email) {
      const emails = await (await fetchImpl('https://api.github.com/user/emails', {headers})).json().catch(() => []);
      email = Array.isArray(emails) ? emails.find(item => item.primary && item.verified)?.email ?? null : null;
    }
    return {subject: String(user.id), name: user.name || user.login || 'GitHub', email, avatar: user.avatar_url || null};
  },
};
