// 保留原模块 URL，因此 import.meta.url、包资源和动态依赖解析都沿用原安装目录。
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const entry = process.argv[2];
const target = pathToFileURL(entry).href;
// 每处补丁必须恰好命中一次；第二处让大脑线程不落盘，不出现在用户的 Codex 会话列表里。
const patches = [
  ["command: stripShellPrefix(params.command)", "command: {normaShellCommand: params.command}"],
  ['method: "thread/start", params }', 'method: "thread/start", params: { ...params, ephemeral: true } }'],
];
try {
const { registerHooks } = await import("node:module");
if (typeof registerHooks !== "function") {
  throw new Error("Codex 权限兼容加载器需要 Node.js 22.15+（registerHooks）");
}
registerHooks({
  load(url, context, nextLoad) {
    const loaded = nextLoad(url, context);
    if (url !== target) return loaded;
    const source = typeof loaded.source === "string"
      ? loaded.source : Buffer.from(loaded.source).toString("utf8");
    let patched = source;
    for (const [needle, replacement] of patches) {
      if (patched.split(needle).length !== 2) {
        throw new Error(`Codex source changed; refusing to start without patch: ${needle}`);
      }
      patched = patched.replace(needle, replacement);
    }
    return { ...loaded, source: patched };
  },
});
process.argv = [process.argv[0], entry, ...process.argv.slice(3)];
await import(target);

} catch (error) {
  // 默认不收集 stderr；启动失败必须沿 ACP 握手返回，不能表现为无输出。
  const lines = createInterface({ input: process.stdin });
  for await (const line of lines) {
    let request;
    try { request = JSON.parse(line); } catch { continue; }
    if (request.id === undefined) continue;
    process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id,
      error: { code: -32603, message: `Codex 兼容加载失败：${error.message}` } }) + "\n");
    lines.close();
    process.stdin.destroy();
    process.exitCode = 1;
    break;
  }
}
