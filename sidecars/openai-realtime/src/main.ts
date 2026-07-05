import { createInterface } from "node:readline";

type JsonObject = Record<string, unknown>;

type NormaEvent =
  | {
      type: "session_started";
      session_id: string;
    }
  | {
      type: "session_error";
      session_id: string;
      message: string;
    }
  | {
      type: "session_stopped";
      session_id: string;
      reason: string;
    };

const unknownSessionId = "unknown";
const apiKey = process.env.OPENAI_API_KEY ?? "";
const readline = createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

let activeSessionId: string | undefined;
let shouldExitAfterDrain = false;
let queue = Promise.resolve();

readline.on("line", (line) => {
  queue = queue.then(() => handleLine(line)).catch(handleUnexpectedError);
});

readline.on("close", () => {
  queue = queue.then(() => {
    if (shouldExitAfterDrain) {
      return;
    }
    process.exitCode = 0;
  });
});

async function handleLine(line: string): Promise<void> {
  const trimmed = line.trim();
  if (trimmed.length === 0) {
    return;
  }

  let command: unknown;
  try {
    command = JSON.parse(trimmed);
  } catch {
    await writeSessionError(activeSessionId ?? unknownSessionId, "malformed JSON line");
    return;
  }

  if (!isJsonObject(command)) {
    await writeSessionError(unknownSessionId, "IPC command must be a JSON object");
    return;
  }

  const commandType = stringField(command, "type");
  if (commandType === undefined) {
    await writeSessionError(sessionIdFrom(command), "IPC command is missing type");
    return;
  }

  if (commandType === "start_session") {
    await handleStartSession(command);
    return;
  }

  if (commandType === "stop_session") {
    await handleStopSession(command);
    return;
  }

  await writeSessionError(sessionIdFrom(command), `unsupported IPC command: ${commandType}`);
}

async function handleStartSession(command: JsonObject): Promise<void> {
  const sessionId = stringField(command, "session_id");
  if (sessionId === undefined) {
    await writeSessionError(unknownSessionId, "start_session is missing session_id");
    return;
  }

  if (offlineEchoEnabled(command)) {
    activeSessionId = sessionId;
    await writeEvent({
      type: "session_started",
      session_id: sessionId,
    });
    return;
  }

  await writeSessionError(
    sessionId,
    "OpenAI realtime WebSocket mode is not implemented in this task; set config.offline_echo=true for offline IPC",
  );
}

async function handleStopSession(command: JsonObject): Promise<void> {
  const sessionId = stringField(command, "session_id") ?? activeSessionId ?? unknownSessionId;
  await writeEvent({
    type: "session_stopped",
    session_id: sessionId,
    reason: "user stopped",
  });

  shouldExitAfterDrain = true;
  readline.close();
  process.stdin.pause();
  process.exit(0);
}

function offlineEchoEnabled(command: JsonObject): boolean {
  const config = command.config;
  if (isJsonObject(config) && config.offline_echo === true) {
    return true;
  }

  return command.offline_echo === true;
}

function sessionIdFrom(command: JsonObject): string {
  return stringField(command, "session_id") ?? activeSessionId ?? unknownSessionId;
}

function stringField(value: JsonObject, field: string): string | undefined {
  const fieldValue = value[field];
  return typeof fieldValue === "string" ? fieldValue : undefined;
}

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

async function writeSessionError(sessionId: string, message: string): Promise<void> {
  await writeEvent({
    type: "session_error",
    session_id: sessionId,
    message: sanitize(message),
  });
}

function writeEvent(event: NormaEvent): Promise<void> {
  return new Promise((resolve, reject) => {
    process.stdout.write(`${JSON.stringify(event)}\n`, (error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

function sanitize(message: string): string {
  let sanitized = message;
  if (apiKey.length > 0) {
    sanitized = sanitized.split(apiKey).join("[redacted]");
  }

  return sanitized.replace(/sk-[A-Za-z0-9_-]+/g, "sk-[redacted]");
}

function handleUnexpectedError(error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  queue = writeSessionError(activeSessionId ?? unknownSessionId, sanitize(message)).catch(() => {
    process.exitCode = 1;
  });
}
