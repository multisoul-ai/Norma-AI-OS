#!/usr/bin/env node

let buffer = "";

function writeJson(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function handleCommand(command) {
  const sessionId = command.session_id || "sidecar-test-session";

  if (command.type === "start_session") {
    if (command.model === "exit-without-event") {
      process.exit(0);
    }

    if (command.model === "emit-malformed-json") {
      process.stdout.write("{malformed-json\n");
      return;
    }

    if (command.model === "emit-unknown-event") {
      writeJson({ type: "mystery", session_id: sessionId });
    }

    writeJson({ type: "session_started", session_id: sessionId });
    return;
  }

  if (command.type === "stop_session") {
    writeJson({
      type: "session_stopped",
      session_id: sessionId,
      reason: "user stopped",
    });
    process.exit(0);
  }

  if (command.type === "input_audio_chunk") {
    writeJson({
      type: "assistant_audio_state",
      session_id: sessionId,
      speaking: true,
    });
    writeJson({
      type: "assistant_audio_chunk",
      session_id: sessionId,
      format: command.format || "pcm16",
      sample_rate: command.sample_rate || 24000,
      channels: command.channels || 1,
      data_b64: command.data_b64 || "AAA=",
    });
    return;
  }

  if (command.type === "interrupt") {
    writeJson({
      type: "assistant_audio_state",
      session_id: sessionId,
      speaking: false,
    });
  }
}

function handleLine(line) {
  if (line.trim() === "") {
    return;
  }

  let command;
  try {
    command = JSON.parse(line);
  } catch {
    process.stderr.write("fake_sidecar received malformed command\n");
    return;
  }

  handleCommand(command);
}

process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  buffer += chunk;
  let newlineIndex = buffer.indexOf("\n");

  while (newlineIndex !== -1) {
    const line = buffer.slice(0, newlineIndex);
    buffer = buffer.slice(newlineIndex + 1);
    handleLine(line);
    newlineIndex = buffer.indexOf("\n");
  }
});

process.stdin.on("end", () => {
  if (buffer.length > 0) {
    handleLine(buffer);
  }
});
