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

function spawnSidecar({ useFakeWebSocket = false } = {}) {
  assert.ok(
    existsSync(sidecarPath),
    `expected compiled sidecar at ${sidecarPath} before running IPC tests`,
  );

  const args = [];
  if (useFakeWebSocket) {
    args.push("--import", resolve(packageRoot, "test/fake-websocket-loader.mjs"));
  }
  args.push(sidecarPath);

  const child = spawn(process.execPath, args, {
    cwd: packageRoot,
    env: { ...process.env, OPENAI_API_KEY: apiKey },
    stdio: ["pipe", "pipe", "pipe", "ipc"],
  });

  const lines = [];
  const waiters = [];
  const childMessages = [];
  const childMessageWaiters = [];
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
  child.on("message", (message) => {
    const waiterIndex = childMessageWaiters.findIndex((waiter) =>
      waiter.predicate(message),
    );
    if (waiterIndex !== -1) {
      const [waiter] = childMessageWaiters.splice(waiterIndex, 1);
      waiter.resolve(message);
      return;
    }
    childMessages.push(message);
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
    async nextEventOfType(type) {
      while (true) {
        const event = await this.nextEvent();
        if (event.type === type) {
          return event;
        }
      }
    },
    async nextChildMessage(kind, predicate = () => true) {
      const existingIndex = childMessages.findIndex(
        (message) => message?.kind === kind && predicate(message),
      );
      if (existingIndex !== -1) {
        return childMessages.splice(existingIndex, 1)[0];
      }

      return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error(`timed out waiting for child IPC message kind=${kind}`));
        }, 1000);

        childMessageWaiters.push({
          predicate(message) {
            return message?.kind === kind && predicate(message);
          },
          resolve(message) {
            clearTimeout(timeout);
            resolve(message);
          },
        });
      });
    },
    sendControl(message) {
      child.send(message);
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

function parseSentRealtimeEvent(message) {
  assert.equal(
    typeof message?.data,
    "string",
    "fake WebSocket telemetry should carry serialized client JSON",
  );
  return JSON.parse(message.data);
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
 * OpenAI start_session handshake: verifies OpenAI mode opens the GA Realtime
 * WebSocket, passes Authorization only in headers, and sends a session.update
 * payload with audio config and task tool definitions.
 *
 * Data construction:
 *   session_id = "rt_openai_start"
 *   model      = "gpt-realtime-2", so the WebSocket URL must target
 *                /v1/realtime?model=gpt-realtime-2.
 *   tools      = ["enqueue_card_task", "cancel_card_task"] to prove the
 *                session.update payload exposes both function definitions.
 *   OPENAI_API_KEY = "sk-test-secret-123456", which must appear only in the
 *                    Authorization header and nowhere in payload/stdout/stderr.
 *
 * Execution process:
 *   1. Spawn the compiled sidecar with `--import test/fake-websocket-loader.mjs`.
 *   2. Send one JSONL `start_session` command for OpenAI mode.
 *   3. Read the fake WebSocket constructor telemetry and assert URL + headers.
 *   4. Simulate socket open and capture the first client event sent upstream.
 *
 * Expected result:
 *   - Positive: URL contains `wss://api.openai.com/v1/realtime?model=gpt-realtime-2`.
 *   - Positive: Authorization header is `Bearer <OPENAI_API_KEY>`.
 *   - Positive: session.update locks audio output to PCM and declares both tools.
 *   - Negative: session.update payload must not contain the raw API key.
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI start_session opens websocket with auth header and sends session.update", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_openai_start",
      config: {
        model: "gpt-realtime-2",
        tools: ["enqueue_card_task", "cancel_card_task"],
      },
    });

    const constructed = await ipc.nextChildMessage("ws-created");

    assert.match(
      constructed.url,
      /^wss:\/\/api\.openai\.com\/v1\/realtime\?model=gpt-realtime-2$/,
      "OpenAI mode should connect to the GA Realtime WebSocket URL for the requested model",
    );
    assert.equal(
      constructed.headers.Authorization,
      `Bearer ${apiKey}`,
      "OpenAI mode must send OPENAI_API_KEY only through the Authorization header",
    );
    ipc.sendControl({ kind: "ws-open" });
    const sent = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));

    assert.equal(
      sent.type,
      "session.update",
      "OpenAI mode should send session.update immediately after the WebSocket opens",
    );
    assert.deepEqual(
      sent.session.output_modalities,
      ["audio", "text"],
      "session.update should request both audio and text output modalities for the voice assistant",
    );
    assert.equal(
      sent.session.audio.input.format.type,
      "audio/pcm",
      "session.update should declare PCM audio input format",
    );
    assert.equal(
      sent.session.audio.input.format.rate,
      24000,
      "session.update should keep the 24 kHz PCM input rate expected by the Rust audio pipeline",
    );
    assert.equal(
      sent.session.audio.output.format.type,
      "audio/pcm",
      "session.update should request PCM audio output for assistant playback",
    );
    assert.equal(
      sent.session.audio.output.voice,
      "marin",
      "session.update should choose a deterministic assistant voice for MVP playback",
    );
    assert.equal(
      sent.session.tools.length,
      2,
      "session.update should expose both enqueue_card_task and cancel_card_task tool definitions",
    );
    assert.deepEqual(
      sent.session.tools.map((tool) => tool.name),
      ["enqueue_card_task", "cancel_card_task"],
      "session.update should include the expected tool names in order",
    );
    assert.doesNotMatch(
      JSON.stringify(sent),
      apiKeyPattern,
      "session.update payload must never embed the raw OPENAI_API_KEY",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI input audio forwarding: verifies one Rust `input_audio_chunk` command
 * becomes one Realtime `input_audio_buffer.append` event with the same base64.
 *
 * Data construction:
 *   session_id = "rt_audio_in"
 *   model      = "gpt-realtime-2"
 *   data_b64   = "AAECAwQ=", representing five deterministic PCM bytes
 *                [0x00, 0x01, 0x02, 0x03, 0x04].
 *
 * Execution process:
 *   1. Start OpenAI mode with the fake WebSocket and simulate socket open.
 *   2. Send one JSONL `input_audio_chunk` command from the Rust side.
 *   3. Inspect the upstream WebSocket payload captured by the fake transport.
 *
 * Expected result:
 *   - Positive: client event type is exactly `input_audio_buffer.append`.
 *   - Positive: upstream `audio` field exactly matches the Rust `data_b64`.
 *   - Negative: forwarded payload must not contain the raw API key.
 */
test("OpenAI input_audio_chunk maps to input_audio_buffer.append", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_audio_in",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });

    ipc.writeJson({
      type: "input_audio_chunk",
      session_id: "rt_audio_in",
      format: "pcm16",
      sample_rate: 24000,
      channels: 1,
      data_b64: "AAECAwQ=",
    });

    const append = parseSentRealtimeEvent(
      await ipc.nextChildMessage("ws-send", (message) => {
        return parseSentRealtimeEvent(message).type === "input_audio_buffer.append";
      }),
    );

    assert.equal(
      append.type,
      "input_audio_buffer.append",
      "Rust input_audio_chunk should forward as OpenAI input_audio_buffer.append",
    );
    assert.equal(
      append.audio,
      "AAECAwQ=",
      "input_audio_buffer.append should preserve the exact base64 PCM bytes from Rust",
    );
    assert.doesNotMatch(
      JSON.stringify(append),
      apiKeyPattern,
      "input_audio_buffer.append payload must never embed the raw OPENAI_API_KEY",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI assistant audio mapping: verifies a fake Realtime audio delta from
 * the WebSocket becomes one Norma `assistant_audio_chunk` event.
 *
 * Data construction:
 *   session_id = "rt_audio_out"
 *   delta      = "AAECAw==", representing four deterministic PCM bytes
 *                [0x00, 0x01, 0x02, 0x03].
 *
 * Execution process:
 *   1. Start OpenAI mode and simulate the WebSocket open handshake.
 *   2. Inject one fake `response.output_audio.delta` server event.
 *   3. Read the corresponding stdout JSONL event from the sidecar.
 *
 * Expected result:
 *   - Positive: event type is exactly `assistant_audio_chunk`.
 *   - Positive: session id is preserved and PCM metadata matches the MVP contract.
 *   - Positive: data_b64 exactly matches the fake OpenAI audio delta.
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI assistant audio delta maps to assistant_audio_chunk", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_audio_out",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.output_audio.delta",
        delta: "AAECAw==",
      },
    });

    const event = await ipc.nextEventOfType("assistant_audio_chunk");

    assert.equal(
      event.session_id,
      "rt_audio_out",
      "assistant_audio_chunk should preserve the active realtime session id",
    );
    assert.equal(
      event.format,
      "pcm16",
      "assistant_audio_chunk should always emit PCM16 to the Rust audio sink",
    );
    assert.equal(
      event.sample_rate,
      24000,
      "assistant_audio_chunk should keep the 24 kHz output sample rate expected by the app",
    );
    assert.equal(
      event.channels,
      1,
      "assistant_audio_chunk should emit mono audio chunks for the MVP playback path",
    );
    assert.equal(
      event.data_b64,
      "AAECAw==",
      "assistant_audio_chunk should preserve the exact OpenAI base64 audio delta",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI error sanitization: verifies server-side Realtime errors become one
 * Norma `session_error` event and redact the raw API key from the message.
 *
 * Data construction:
 *   session_id = "rt_error"
 *   fake error = "upstream rejected sk-test-secret-123456"
 *
 * Execution process:
 *   1. Start OpenAI mode and simulate the WebSocket open handshake.
 *   2. Inject one fake `error` server event whose message contains the API key.
 *   3. Read the resulting stdout JSONL `session_error`.
 *
 * Expected result:
 *   - Positive: event type is exactly `session_error`.
 *   - Positive: message still explains the upstream failure.
 *   - Negative: message, stdout, and stderr must not contain the raw API key.
 */
test("OpenAI error maps to sanitized session_error", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_error",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "error",
        error: {
          message: `upstream rejected ${apiKey}`,
        },
      },
    });

    const event = await ipc.nextEventOfType("session_error");

    assert.equal(
      event.session_id,
      "rt_error",
      "session_error should preserve the active realtime session id",
    );
    assert.match(
      event.message,
      /upstream rejected/i,
      "session_error should keep a useful upstream failure summary",
    );
    assert.doesNotMatch(
      event.message,
      apiKeyPattern,
      "session_error must redact the raw OPENAI_API_KEY from error messages",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});
