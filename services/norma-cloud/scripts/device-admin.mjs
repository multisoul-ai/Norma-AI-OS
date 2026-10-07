// Run only on the gateway host; never put the administrator token in the iPhone app.
const [command, deviceID] = process.argv.slice(2);
const token = process.env.COMPANION_ACCESS_TOKEN;
if (!token || token.length < 24 || !['invite', 'list', 'revoke'].includes(command) ||
    (command === 'revoke' && !deviceID)) {
  console.error('Usage: COMPANION_ACCESS_TOKEN=… node scripts/device-admin.mjs invite|list|revoke <device-id>');
  process.exit(2);
}
const port = Number(process.env.PORT || 8787);
const path = command === 'invite' ? 'invite' : command === 'list' ? 'list' : 'revoke';
const response = await fetch(`http://127.0.0.1:${port}/v1/admin/devices/${path}`, {
  method: 'POST', headers: {'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json'},
  body: JSON.stringify(command === 'revoke' ? {device_id: deviceID} : {}),
});
const value = await response.json();
if (!response.ok) { console.error(value.error || `HTTP ${response.status}`); process.exit(1); }
if (command === 'invite') console.log(`norma://invite?code=${value.code}`);
else console.log(JSON.stringify(value, null, 2));
