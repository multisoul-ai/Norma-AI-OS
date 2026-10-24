// Google OAuth 提供方。scope 为 openid email profile。
export const google = {
  id: 'google',
  authorizeUrl: 'https://accounts.google.com/o/oauth2/v2/auth',
  scope: 'openid email profile',
  tokenRequest({code, redirectUri, clientId, clientSecret}) {
    return {
      url: 'https://oauth2.googleapis.com/token',
      init: {method: 'POST', headers: {'content-type': 'application/x-www-form-urlencoded'},
        body: new URLSearchParams({client_id: clientId, client_secret: clientSecret, code,
          redirect_uri: redirectUri, grant_type: 'authorization_code'}).toString()},
    };
  },
  async fetchProfile(fetchImpl, accessToken) {
    const info = await (await fetchImpl('https://openidconnect.googleapis.com/v1/userinfo',
      {headers: {authorization: `Bearer ${accessToken}`}})).json();
    if (!info?.sub) throw new Error('google profile missing sub');
    return {subject: String(info.sub), name: info.name || 'Google', email: info.email || null, avatar: info.picture || null};
  },
};
