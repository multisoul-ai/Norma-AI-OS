import {randomBytes} from 'node:crypto';
import {existsSync, mkdirSync, readFileSync, renameSync, writeFileSync} from 'node:fs';
import {dirname} from 'node:path';

const key = (provider, subject) => `${provider}:${subject}`;

// 文件实现：0600、原子写入；损坏文件直接抛错，绝不静默重置。
export function createFileAccountStore(path) {
  const data = existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : {accounts: {}, sessions: {}};
  if (!data.accounts || !data.sessions) throw new Error('account store is corrupt');
  const byIdentity = new Map();
  for (const account of Object.values(data.accounts)) {
    for (const {provider, subject} of account.identities) byIdentity.set(key(provider, subject), account.account_id);
  }
  const save = () => {
    mkdirSync(dirname(path), {recursive: true});
    const temp = `${path}.${process.pid}.tmp`;
    writeFileSync(temp, JSON.stringify(data), {mode: 0o600});
    renameSync(temp, path);
  };
  return {
    findByIdentity(provider, subject) {
      const id = byIdentity.get(key(provider, subject));
      return id ? data.accounts[id] : null;
    },
    upsertFromLogin({provider, subject, name, email, avatar}) {
      let account = this.findByIdentity(provider, subject);
      if (!account) {
        account = {account_id: `acc_${randomBytes(16).toString('hex')}`, identities: [{provider, subject}], created_at: Date.now()};
        data.accounts[account.account_id] = account;
        byIdentity.set(key(provider, subject), account.account_id);
      }
      Object.assign(account, {display_name: name, email: email ?? null, avatar_url: avatar ?? null});
      save();
      return account;
    },
    get: id => data.accounts[id] ?? null,
    putSession(session) { data.sessions[session.sid] = session; save(); },
    getSession: sid => data.sessions[sid] ?? null,
    deleteSession(sid) { if (delete data.sessions[sid]) save(); },
  };
}
