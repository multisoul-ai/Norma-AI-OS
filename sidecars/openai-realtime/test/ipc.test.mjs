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
        }, 2000);

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
    }, 2000);

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
 * OpenAI interrupt truncation: verifies interrupt sends cancel, truncate, and
 * clear in order after the sidecar has seen one assistant item id and one
 * deterministic audio delta duration.
 *
 * Data construction:
 *   session_id = "rt_interrupt"
 *   item_id    = "asst_item_1"
 *   audio bytes = 960 bytes of PCM16 silence.
 *   audio_end_ms = bytes(960) / bytes_per_ms(24000 Hz * 1 ch * 2 B = 48) = 20 ms.
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Inject one fake assistant audio delta carrying `item_id="asst_item_1"`.
 *   3. Send one Rust `interrupt` command for the active session.
 *   4. Inspect the next three upstream WebSocket payloads and the local audio-state event.
 *
 * Expected result:
 *   - Positive: first payload is `response.cancel`.
 *   - Positive: second payload is `conversation.item.truncate` for `asst_item_1` with `audio_end_ms=20`.
 *   - Positive: third payload is `input_audio_buffer.clear`.
 *   - Positive: local `assistant_audio_state` becomes `false`.
 *   - Negative: none of the payloads contain the raw API key.
 */
test("OpenAI interrupt sends cancel truncate clear sequence when assistant item is known", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_interrupt",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.output_audio.delta",
        item_id: "asst_item_1",
        delta: Buffer.alloc(960).toString("base64"),
      },
    });
    const speakingTrue = await ipc.nextEventOfType("assistant_audio_state");
    const audioChunk = await ipc.nextEventOfType("assistant_audio_chunk");

    assert.equal(
      speakingTrue.speaking,
      true,
      "assistant audio delta should first mark assistant_audio_state speaking=true during test setup",
    );
    assert.equal(
      audioChunk.data_b64,
      Buffer.alloc(960).toString("base64"),
      "assistant audio delta should preserve the deterministic base64 payload during test setup",
    );

    ipc.writeJson({ type: "interrupt", session_id: "rt_interrupt" });

    const cancel = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));
    const truncate = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));
    const clear = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));
    const speakingFalse = await ipc.nextEventOfType("assistant_audio_state");

    assert.equal(
      cancel.type,
      "response.cancel",
      "interrupt should send response.cancel before any truncate or clear payload",
    );
    assert.equal(
      truncate.type,
      "conversation.item.truncate",
      "interrupt should send conversation.item.truncate after response.cancel when an assistant item id is known",
    );
    assert.equal(
      truncate.item_id,
      "asst_item_1",
      "conversation.item.truncate should target the latest known assistant item id",
    );
    assert.equal(
      truncate.content_index,
      0,
      "conversation.item.truncate should target the first assistant content part for MVP audio playback",
    );
    assert.equal(
      truncate.audio_end_ms,
      20,
      "conversation.item.truncate should use the deterministic 20 ms audio duration implied by the fake PCM chunk",
    );
    assert.equal(
      clear.type,
      "input_audio_buffer.clear",
      "interrupt should clear the input audio buffer after cancel and truncate",
    );
    assert.equal(
      speakingFalse.speaking,
      false,
      "interrupt should emit assistant_audio_state speaking=false locally after the upstream interrupt sequence",
    );
    assert.doesNotMatch(
      JSON.stringify(cancel),
      apiKeyPattern,
      "response.cancel payload must never embed the raw OPENAI_API_KEY",
    );
    assert.doesNotMatch(
      JSON.stringify(truncate),
      apiKeyPattern,
      "conversation.item.truncate payload must never embed the raw OPENAI_API_KEY",
    );
    assert.doesNotMatch(
      JSON.stringify(clear),
      apiKeyPattern,
      "input_audio_buffer.clear payload must never embed the raw OPENAI_API_KEY",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI interrupt fallback: verifies interrupt omits truncate when no
 * assistant item id has been observed yet.
 *
 * Data construction:
 *   session_id = "rt_interrupt_no_item"
 *   no assistant server events are injected before interrupt, so the sidecar
 *   has no safe assistant item id to truncate.
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Send one Rust `interrupt` command immediately.
 *   3. Inspect the next two upstream WebSocket payloads and the local audio-state event.
 *
 * Expected result:
 *   - Positive: first payload is `response.cancel`.
 *   - Positive: second payload is `input_audio_buffer.clear`.
 *   - Positive: local `assistant_audio_state` becomes `false`.
 *   - Negative: no `conversation.item.truncate` payload is sent.
 */
test("OpenAI interrupt skips truncate when no assistant item is known", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_interrupt_no_item",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.writeJson({ type: "interrupt", session_id: "rt_interrupt_no_item" });

    const first = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));
    const second = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));
    const speakingFalse = await ipc.nextEventOfType("assistant_audio_state");

    assert.equal(
      first.type,
      "response.cancel",
      "interrupt should still send response.cancel even before any assistant item is known",
    );
    assert.equal(
      second.type,
      "input_audio_buffer.clear",
      "interrupt should clear the input audio buffer directly when truncate is unavailable",
    );
    assert.notEqual(
      second.type,
      "conversation.item.truncate",
      "interrupt fallback must not fabricate a conversation.item.truncate without a known assistant item id",
    );
    assert.equal(
      speakingFalse.speaking,
      false,
      "interrupt fallback should still emit assistant_audio_state speaking=false locally",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI assistant text mapping: verifies both transcript-oriented and plain
 * text response deltas become Norma `assistant_text_delta` events.
 *
 * Data construction:
 *   session_id = "rt_text_delta"
 *   delta A    = "Hello"
 *   delta B    = " world"
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Inject `response.output_audio_transcript.delta` with "Hello".
 *   3. Inject `response.output_text.delta` with " world".
 *   4. Read the two stdout JSONL assistant-text events.
 *
 * Expected result:
 *   - Positive: both events are `assistant_text_delta`.
 *   - Positive: first text is "Hello" and second text is " world".
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI transcript and text deltas map to assistant_text_delta", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_text_delta",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.output_audio_transcript.delta",
        delta: "Hello",
      },
    });
    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.output_text.delta",
        delta: " world",
      },
    });

    const first = await ipc.nextEventOfType("assistant_text_delta");
    const second = await ipc.nextEventOfType("assistant_text_delta");

    assert.equal(
      first.text,
      "Hello",
      "response.output_audio_transcript.delta should map to assistant_text_delta with the original delta text",
    );
    assert.equal(
      second.text,
      " world",
      "response.output_text.delta should map to assistant_text_delta with the original delta text",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI input transcription mapping: verifies one input-audio transcription
 * delta becomes one Norma `transcript_delta` event.
 *
 * Data construction:
 *   session_id = "rt_transcript_delta"
 *   delta      = "ni"
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Inject one fake `conversation.item.input_audio_transcription.delta`.
 *   3. Read the resulting stdout JSONL event.
 *
 * Expected result:
 *   - Positive: event type is exactly `transcript_delta`.
 *   - Positive: transcript text is exactly "ni".
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI input transcription delta maps to transcript_delta", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_transcript_delta",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "conversation.item.input_audio_transcription.delta",
        delta: "ni",
      },
    });

    const event = await ipc.nextEventOfType("transcript_delta");

    assert.equal(
      event.text,
      "ni",
      "conversation.item.input_audio_transcription.delta should map to transcript_delta with the original delta text",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI stop_session lifecycle: verifies OpenAI mode closes the active
 * WebSocket, emits `session_stopped`, and exits cleanly.
 *
 * Data construction:
 *   session_id = "rt_openai_stop"
 *   close code = 1000, reason = "user stopped" from the sidecar's explicit stop path.
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Send one Rust `stop_session` command.
 *   3. Read the fake WebSocket close telemetry, the stop event, and the exit code.
 *
 * Expected result:
 *   - Positive: fake transport records one client close with code 1000 and reason "user stopped".
 *   - Positive: stdout emits `session_stopped` for the active session.
 *   - Positive: process exits with code 0.
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI stop_session closes websocket emits session_stopped and exits", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_openai_stop",
      config: { model: "gpt-realtime-2", tools: [] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.writeJson({ type: "stop_session", session_id: "rt_openai_stop" });

    const closed = await ipc.nextChildMessage("ws-close");
    const stopped = await ipc.nextEventOfType("session_stopped");
    const exitCode = await ipc.waitForExit();

    assert.equal(
      closed.code,
      1000,
      "OpenAI stop_session should close the WebSocket with the normal code 1000",
    );
    assert.equal(
      closed.reason,
      "user stopped",
      "OpenAI stop_session should close the WebSocket with the explicit user-stopped reason",
    );
    assert.equal(
      stopped.session_id,
      "rt_openai_stop",
      "session_stopped should preserve the active OpenAI session id during stop_session",
    );
    assert.equal(
      stopped.reason,
      "user stopped",
      "session_stopped should explain that the user requested the stop",
    );
    assert.equal(exitCode, 0, "OpenAI stop_session should exit the sidecar cleanly");
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

/**
 * OpenAI function-call output-item mapping: verifies one completed
 * `response.output_item.done` function call becomes one Norma `tool_call`.
 *
 * Data construction:
 *   session_id    = "rt_tool_call_enqueue"
 *   tool_call_id  = "call_1"
 *   function name = "enqueue_card_task"
 *   arguments     = {"intent":"创建 Codex 卡片"}
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Inject one fake `response.output_item.done` event whose item is a function call.
 *   3. Read the resulting stdout JSONL `tool_call`.
 *
 * Expected result:
 *   - Positive: event type is exactly `tool_call`.
 *   - Positive: tool_call_id/name/arguments match the upstream function call.
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI response.output_item.done function call maps to tool_call", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_tool_call_enqueue",
      config: { model: "gpt-realtime-2", tools: ["enqueue_card_task"] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.output_item.done",
        item: {
          type: "function_call",
          call_id: "call_1",
          name: "enqueue_card_task",
          arguments: JSON.stringify({ intent: "创建 Codex 卡片" }),
        },
      },
    });

    const event = await ipc.nextEventOfType("tool_call");

    assert.equal(
      event.session_id,
      "rt_tool_call_enqueue",
      "tool_call should preserve the active realtime session id",
    );
    assert.equal(
      event.tool_call_id,
      "call_1",
      "tool_call should preserve the upstream function call id",
    );
    assert.equal(
      event.name,
      "enqueue_card_task",
      "tool_call should preserve the upstream function name",
    );
    assert.deepEqual(
      event.arguments,
      { intent: "创建 Codex 卡片" },
      "tool_call should decode the upstream JSON arguments into a plain object",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI function-call arguments-done mapping: verifies one completed
 * `response.function_call_arguments.done` event becomes one Norma `tool_call`.
 *
 * Data construction:
 *   session_id    = "rt_tool_call_cancel"
 *   tool_call_id  = "call_cancel_1"
 *   function name = "cancel_card_task"
 *   arguments     = {"scope":"latest"}
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Inject one fake `response.function_call_arguments.done` event.
 *   3. Read the resulting stdout JSONL `tool_call`.
 *
 * Expected result:
 *   - Positive: event type is exactly `tool_call`.
 *   - Positive: tool_call_id/name/arguments match the upstream function call.
 *   - Negative: stdout and stderr must not contain the raw API key.
 */
test("OpenAI response.function_call_arguments.done maps to cancel tool_call", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_tool_call_cancel",
      config: { model: "gpt-realtime-2", tools: ["cancel_card_task"] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.function_call_arguments.done",
        call_id: "call_cancel_1",
        name: "cancel_card_task",
        arguments: JSON.stringify({ scope: "latest" }),
      },
    });

    const event = await ipc.nextEventOfType("tool_call");

    assert.equal(
      event.tool_call_id,
      "call_cancel_1",
      "tool_call should preserve the function call id from arguments.done events",
    );
    assert.equal(
      event.name,
      "cancel_card_task",
      "tool_call should preserve the cancel function name",
    );
    assert.deepEqual(
      event.arguments,
      { scope: "latest" },
      "tool_call should decode JSON arguments from arguments.done events",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI invalid function-call arguments: verifies malformed JSON arguments do
 * not become sidecar errors and instead emit a fallback Norma `tool_call`.
 *
 * Data construction:
 *   session_id    = "rt_tool_call_invalid_args"
 *   tool_call_id  = "call_bad_args"
 *   function name = "enqueue_card_task"
 *   arguments     = "{bad json" (deterministic malformed payload)
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Inject one fake function-call event with malformed JSON arguments.
 *   3. Read the resulting stdout JSONL `tool_call`.
 *
 * Expected result:
 *   - Positive: event type is exactly `tool_call`.
 *   - Positive: arguments fall back to `{}` so Rust can emit a failed tool result.
 *   - Negative: the sidecar must not emit `session_error` for malformed arguments.
 */
test("OpenAI malformed function-call arguments still emit tool_call with empty arguments", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_tool_call_invalid_args",
      config: { model: "gpt-realtime-2", tools: ["enqueue_card_task"] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.sendControl({
      kind: "ws-message",
      data: {
        type: "response.output_item.done",
        item: {
          type: "function_call",
          id: "call_bad_args",
          name: "enqueue_card_task",
          arguments: "{bad json",
        },
      },
    });

    const event = await ipc.nextEventOfType("tool_call");

    assert.equal(
      event.tool_call_id,
      "call_bad_args",
      "tool_call should fall back to item.id when call_id is unavailable",
    );
    assert.deepEqual(
      event.arguments,
      {},
      "malformed JSON arguments should degrade to an empty object for Rust-side validation",
    );
    assert.doesNotMatch(
      JSON.stringify(event),
      apiKeyPattern,
      "tool_call fallback payload must never embed the raw OPENAI_API_KEY",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});

/**
 * OpenAI tool_result forwarding: verifies one Rust `tool_result` command
 * becomes one `conversation.item.create(function_call_output)` followed by one
 * `response.create`.
 *
 * Data construction:
 *   session_id   = "rt_tool_result"
 *   tool_call_id = "call_failed_1"
 *   result       = {"status":"failed","summary":"missing intent"}
 *
 * Execution process:
 *   1. Start OpenAI mode and finish the fake WebSocket handshake.
 *   2. Send one JSONL `tool_result` command from Rust.
 *   3. Inspect the next two upstream WebSocket payloads.
 *
 * Expected result:
 *   - Positive: first payload is `conversation.item.create` with `function_call_output`.
 *   - Positive: second payload is `response.create`.
 *   - Negative: neither payload contains the raw API key.
 */
test("OpenAI tool_result maps to function_call_output then response.create", async () => {
  const ipc = spawnSidecar({ useFakeWebSocket: true });
  try {
    ipc.writeJson({
      type: "start_session",
      session_id: "rt_tool_result",
      config: { model: "gpt-realtime-2", tools: ["enqueue_card_task"] },
    });
    await ipc.nextChildMessage("ws-created");
    ipc.sendControl({ kind: "ws-open" });
    await ipc.nextChildMessage("ws-send", (message) => {
      return parseSentRealtimeEvent(message).type === "session.update";
    });
    await ipc.nextEventOfType("session_started");

    ipc.writeJson({
      type: "tool_result",
      session_id: "rt_tool_result",
      tool_call_id: "call_failed_1",
      result: {
        status: "failed",
        summary: "missing intent",
      },
    });

    const itemCreate = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));
    const responseCreate = parseSentRealtimeEvent(await ipc.nextChildMessage("ws-send"));

    assert.equal(
      itemCreate.type,
      "conversation.item.create",
      "tool_result should first inject a conversation.item.create event",
    );
    assert.equal(
      itemCreate.item.type,
      "function_call_output",
      "tool_result should inject a function_call_output item for the matching tool call",
    );
    assert.equal(
      itemCreate.item.call_id,
      "call_failed_1",
      "function_call_output should target the original tool_call_id",
    );
    assert.deepEqual(
      JSON.parse(itemCreate.item.output),
      {
        status: "failed",
        summary: "missing intent",
      },
      "function_call_output should serialize the structured Rust tool result payload",
    );
    assert.equal(
      responseCreate.type,
      "response.create",
      "tool_result should request a follow-up assistant response after injecting function_call_output",
    );
    assert.doesNotMatch(
      JSON.stringify(itemCreate),
      apiKeyPattern,
      "function_call_output payload must never embed the raw OPENAI_API_KEY",
    );
    assert.doesNotMatch(
      JSON.stringify(responseCreate),
      apiKeyPattern,
      "response.create payload must never embed the raw OPENAI_API_KEY",
    );
    assertNoApiKeyLeak(ipc);
  } finally {
    await ipc.cleanup();
  }
});
