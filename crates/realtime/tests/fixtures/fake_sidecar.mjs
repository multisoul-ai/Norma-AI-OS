#!/usr/bin/env node

let buffer = "";

function writeJson(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function handleCommand(command) {
  const sessionId = command.session_id || "sidecar-test-session";

  if (command.type === "start_session") {
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
