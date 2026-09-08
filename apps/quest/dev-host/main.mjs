import { startHost } from './server.mjs';

const port = Number(process.env.NORMA_QUEST_PORT ?? 8787);
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('NORMA_QUEST_PORT must be 1–65535');
const host = await startHost({ port });
console.log(`Norma Quest mock host: ${host.url}`);
console.log(`通过 USB 调试连接 Quest：adb reverse tcp:${port} tcp:${port}`);
console.log('仅模拟会话；不会运行 Agent、shell、构建或发布。');
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, async () => { await host.close(); process.exit(0); });
}
