import {github} from './github.mjs';
import {google} from './google.mjs';

// 提供方注册表：新增提供方只需新增文件并在此注册，路由与配置加载无需改动。
export const defaultProviders = {github, google};

// 用授权码换提供方 token，读取一次 profile 后丢弃 token。
export async function completeLogin(provider, {code, redirectUri, clientId, clientSecret}, fetchImpl) {
  const {url, init} = provider.tokenRequest({code, redirectUri, clientId, clientSecret});
  const response = await fetchImpl(url, init);
  const body = await response.json();
  if (!response.ok || !body?.access_token) throw new Error('provider token exchange failed');
  return provider.fetchProfile(fetchImpl, body.access_token);
}
