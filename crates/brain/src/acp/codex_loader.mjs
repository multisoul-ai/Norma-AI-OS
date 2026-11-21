// 保留原模块 URL，因此 import.meta.url、包资源和动态依赖解析都沿用原安装目录。
import { spawnSync } from "node:child_process";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const entry = process.argv[2];
const target = pathToFileURL(entry).href;
// 每处补丁必须恰好命中一次；第二处让大脑线程不落盘，不出现在用户的 Codex 会话列表里。
const patches = [
  ["command: stripShellPrefix(params.command)", "command: {normaShellCommand: params.command}"],
  ['method: "thread/start", params }', 'method: "thread/start", params: { ...params, ephemeral: true } }'],
  // 共用进程时，每个会话的 token 从 _meta.norma.config 浅合并进 thread/start，不进进程环境。
  [
    "const response = await this.codexClient.threadStart({\n      config: sessionConfig.config,",
    "const response = await this.codexClient.threadStart({\n      config: { ...sessionConfig.config, ...(((request._meta || {}).norma || {}).config || {}) },",
  ],
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
// ACP 2.2.2 随包的 Codex 0.160.1 缺少较新模型的元数据。显式路径优先。
if (process.env.CODEX_PATH === undefined) {
  const probe = spawnSync("codex", ["--version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
    timeout: 2000,
    killSignal: "SIGKILL",
  });
  const version = /^codex-cli 0\.(\d+)\.(\d+)\s*$/.exec(probe.stdout ?? "");
  if (probe.status === 0 && version) {
    const [, minor, patch] = version.map(Number);
    if (minor > 160 || (minor === 160 && patch > 1)) {
      process.env.CODEX_PATH = "codex";
    }
  }
}
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
