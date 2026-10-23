// AccountStore 接口约定（当前实现见 file-store.mjs，后续可换数据库而不改路由）：
//   findByIdentity(provider, subject) -> account | null
//   upsertFromLogin({provider, subject, name, email, avatar}) -> account   同一 (provider, subject) 永远得到同一 account_id
//   get(accountId) -> account | null
//   putSession({sid, accountId, refreshHash, expiresAt}) / getSession(sid) / deleteSession(sid)
// 身份只由 (provider, subject) 决定；邮箱仅作展示，绝不参与查找或合并。

export function toProfile(account) {
  return {
    account_id: account.account_id,
    display_name: account.display_name,
    email: account.email ?? null,
    avatar_url: account.avatar_url ?? null,
    identities: account.identities.map(({provider, subject}) => ({provider, subject})),
  };
}
