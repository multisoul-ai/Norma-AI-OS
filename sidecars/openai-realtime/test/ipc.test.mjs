import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sidecarPath = resolve(packageRoot, "dist/main.js");
const apiKey = "sk-test-secret-123456";
const apiKeyPattern = /sk-test-secret-123456/;

function spawnSidecar() {
  assert.ok(
    existsSync(sidecarPath),
    `expected compiled sidecar at ${sidecarPath} before running IPC tests`,
  );

  const child = spawn(process.execPath, [sidecarPath], {
    cwd: packageRoot,
    env: { ...process.env, OPENAI_API_KEY: apiKey },
    stdio: ["pipe", "pipe", "pipe"],
  });

  const lines = [];
  const waiters = [];
  const stdoutChunks = [];
  const stderrChunks = [];
  let stdoutBuffer = "";

  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdoutChunks.push(chunk);
    stdoutBuffer += chunk;

    let newlineIndex = stdoutBuffer.indexOf("\n");
    while (newlineIndex !== -1) {
      const line = stdoutBuffer.slice(0, newlineIndex);
      stdoutBuffer = stdoutBuffer.slice(newlineIndex + 1);
      if (line.trim().length > 0) {
        const waiter = waiters.shift();
        if (waiter) {
          waiter.resolve(line);
        } else {
          lines.push(line);
        }
      }
      newlineIndex = stdoutBuffer.indexOf("\n");
    }
  });
  child.stderr.on("data", (chunk) => {
    stderrChunks.push(chunk);
  });

  return {
    child,
    writeJson(value) {
      child.stdin.write(`${JSON.stringify(value)}\n`);
    },
    writeRaw(line) {
      child.stdin.write(`${line}\n`);
    },
    async nextEvent() {
      const line = await nextLine(lines, waiters);
      return JSON.parse(line);
    },
    stdoutText() {
      return stdoutChunks.join("");
    },
    stderrText() {
      return stderrChunks.join("");
    },
    async waitForExit() {
      if (child.exitCode !== null) {
        return child.exitCode;
      }
      const [code] = await Promise.race([
        once(child, "exit"),
        rejectAfter(1000, "timed out waiting for sidecar to exit"),
      ]);
      return code;
    },
    async cleanup() {
      if (child.exitCode !== null) {
        return;
      }
      child.kill();
      await Promise.race([
        once(child, "exit"),
        rejectAfter(1000, "timed out waiting for sidecar cleanup"),
      ]);
    },
  };
}

function nextLine(lines, waiters) {
  if (lines.length > 0) {
    return Promise.resolve(lines.shift());
  }

  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error("timed out waiting for sidecar stdout JSONL"));
    }, 1000);

    waiters.push({
      resolve(line) {
        clearTimeout(timeout);
        resolve(line);
      },
    });
  });
}

function rejectAfter(timeoutMs, message) {
  return new Promise((_, reject) => {
    setTimeout(() => reject(new Error(message)), timeoutMs);
  });
}

function assertNoApiKeyLeak(ipc) {
  assert.doesNotMatch(
    ipc.stdoutText(),
    apiKeyPattern,
    "stdout must not contain the raw OPENAI_API_KEY",
  );
  assert.doesNotMatch(
    ipc.stderrText(),
    apiKeyPattern,
    "stderr must not contain the raw OPENAI_API_KEY",
  );
}

/**
 * start_session offline echo: verifies the sidecar accepts the Task 4 test
 * config shape and emits a Norma `session_started` event.
 *
 * Data construction:
 *   session_id    = "rt_offline"
 *   offline_echo  = true, so no OpenAI network path is needed.
 *   OPENAI_API_KEY = "sk-test-secret-123456", used only to prove redaction.
 *
 * Execution process:
 *   1. Spawn `node dist/main.js`.
 *   2. Send one JSONL `start_session` command with `config.offline_echo=true`.
 *   3. Read one stdout JSONL event.
 *
 * Expected result:
 *   - Positive: event type is exactly `session_started`.
 *   - Positive: event session id is exactly `rt_offline`.
 *   - Negative: stdout and stderr do not contain the API key.
 */
test("start_session with offline_echo returns session_started", async () => {
  const ipc = spawnSidecar();
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_offline",
      config: {
        model: "gpt-realtime",
        tools: ["enqueue_card_task", "cancel_card_task"],
        offline_echo: true,
      },
    });

    const event = await ipc.nextEvent();

    assert.equal(
      event.type,
      "session_started",
      "offline start_session should emit session_started",
    );
    assert.equal(
      event.session_id,
      "rt_offline",
      "session_started should preserve the command session_id",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * stop_session lifecycle: verifies a running offline sidecar acknowledges stop
 * and then exits so Rust can wait without killing the process.
 *
 * Data construction:
 *   session_id = "rt_stop"
 *   offline_echo = true starts the deterministic local mode.
 *
 * Execution process:
 *   1. Start an offline session and consume `session_started`.
 *   2. Send one JSONL `stop_session` command for the same session.
 *   3. Read one stdout JSONL event, then wait for process exit.
 *
 * Expected result:
 *   - Positive: stop event type is exactly `session_stopped`.
 *   - Positive: stop event session id is exactly `rt_stop`.
 *   - Positive: sidecar exits with code 0 after writing the stop event.
 *   - Negative: stdout and stderr do not contain the API key.
 */
test("stop_session returns session_stopped and exits", async () => {
  const ipc = spawnSidecar();
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_stop",
      config: { model: "gpt-realtime", tools: [], offline_echo: true },
    });
    const started = await ipc.nextEvent();
    assert.equal(
      started.type,
      "session_started",
      "test setup should start the offline session before stopping",
    );

    ipc.writeJson({ type: "stop_session", session_id: "rt_stop" });
    const stopped = await ipc.nextEvent();
    const exitCode = await ipc.waitForExit();

    assert.equal(
      stopped.type,
      "session_stopped",
      "stop_session should emit session_stopped before exit",
    );
    assert.equal(
      stopped.session_id,
      "rt_stop",
      "session_stopped should preserve the stopped session_id",
    );
    assert.equal(exitCode, 0, "sidecar should exit cleanly after stop_session");
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * malformed input handling: verifies invalid JSON becomes a protocol error
 * event instead of an uncaught exception or process crash.
 *
 * Data construction:
 *   raw line = "{\"type\":\"start_session\"" with no closing brace.
 *   fallback session_id = "unknown" because the malformed line has no safe id.
 *
 * Execution process:
 *   1. Spawn `node dist/main.js`.
 *   2. Write the malformed JSONL line.
 *   3. Read one stdout JSONL event and inspect process state.
 *
 * Expected result:
 *   - Positive: event type is exactly `session_error`.
 *   - Positive: malformed input uses the explicit fallback session id.
 *   - Positive: process is still running after emitting the error.
 *   - Negative: stdout and stderr do not contain the API key.
 */
test("malformed input returns session_error without crashing", async () => {
  const ipc = spawnSidecar();
  try {
    ipc.writeRaw('{"type":"start_session"');

    const event = await ipc.nextEvent();

    assert.equal(
      event.type,
      "session_error",
      "malformed JSON should emit session_error",
    );
    assert.equal(
      event.session_id,
      "unknown",
      "malformed JSON should use the explicit unknown session fallback",
    );
    assert.match(
      event.message,
      /malformed JSON/i,
      "session_error should explain that the JSONL line was malformed",
    );
    assert.equal(
      ipc.child.exitCode,
      null,
      "sidecar should remain alive after a malformed input error",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI mode placeholder: verifies Task 4 does not implement a WebSocket
 * client and instead returns a clear error when offline_echo is absent.
 *
 * Data construction:
 *   session_id = "rt_openai"
 *   config.offline_echo is omitted, so this requests future OpenAI mode.
 *
 * Execution process:
 *   1. Spawn `node dist/main.js`.
 *   2. Send one JSONL `start_session` command without `offline_echo`.
 *   3. Read one stdout JSONL event.
 *
 * Expected result:
 *   - Positive: event type is exactly `session_error`.
 *   - Positive: event message states OpenAI realtime is not implemented.
 *   - Negative: no `session_started` event is emitted for OpenAI mode.
 *   - Negative: stdout and stderr do not contain the API key.
 */
test("OpenAI mode returns clear session_error until WebSocket support exists", async () => {
  const ipc = spawnSidecar();
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_openai",
      config: { model: "gpt-realtime", tools: ["enqueue_card_task"] },
    });

    const event = await ipc.nextEvent();

    assert.equal(
      event.type,
      "session_error",
      "non-offline start_session should emit session_error in Task 4",
    );
    assert.equal(
      event.session_id,
      "rt_openai",
      "OpenAI-mode error should preserve the command session_id",
    );
    assert.match(
      event.message,
      /not implemented/i,
      "OpenAI-mode error should clearly state realtime WebSocket is not implemented",
    );
    assert.notEqual(
      event.type,
      "session_started",
      "OpenAI mode must not emit session_started before WebSocket support exists",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});
