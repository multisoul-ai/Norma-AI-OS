import { createInterface } from "node:readline";
import { WebSocket as WsWebSocket } from "ws";
import type { ClientOptions } from "ws";

type JsonObject = Record<string, unknown>;

type NormaEvent =
  | {
      type: "session_started";
      session_id: string;
    }
  | {
      type: "transcript_delta";
      session_id: string;
      text: string;
    }
  | {
      type: "transcript_final";
      session_id: string;
      text: string;
    }
  | {
      type: "assistant_text_delta";
      session_id: string;
      text: string;
    }
  | {
      type: "assistant_audio_chunk";
      session_id: string;
      format: "pcm16";
      sample_rate: 24000;
      channels: 1;
      data_b64: string;
    }
  | {
      type: "assistant_audio_state";
      session_id: string;
      speaking: boolean;
    }
  | {
      type: "tool_call";
      session_id: string;
      tool_call_id: string;
      name: string;
      arguments: JsonObject;
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

type WebSocketLike = {
  on(event: "open", listener: () => void): unknown;
  on(event: "error", listener: (error: unknown) => void): unknown;
  on(event: "close", listener: (code: number, reason: Buffer) => void): unknown;
  on(event: "message", listener: (data: unknown, isBinary: boolean) => void): unknown;
  send(data: string, callback?: (error?: Error) => void): unknown;
  close(code?: number, reason?: string): unknown;
  readyState: number;
};

type WebSocketConstructor = new (
  url: string,
  options?: ClientOptions,
) => WebSocketLike;

type ActiveSession = {
  sessionId: string;
  socket: WebSocketLike;
  started: boolean;
  stoppingForUser: boolean;
  settledAfterClose: boolean;
  latestAssistantItemId: string | undefined;
  latestAssistantAudioEndMs: number;
  openDeferred:
    | {
        resolve: () => void;
        reject: (error: Error) => void;
      }
    | undefined;
};

type ToolDefinition = {
  type: "function";
  name: "enqueue_card_task" | "cancel_card_task";
  description: string;
  parameters: JsonObject;
};

type OpenAiClientEvent =
  | {
      type: "session.update";
      session: JsonObject;
    }
  | {
      type: "input_audio_buffer.append";
      audio: string;
    }
  | {
      type: "input_audio_buffer.clear";
    }
  | {
      type: "response.cancel";
    }
  | {
      type: "conversation.item.truncate";
      item_id: string;
      content_index: number;
      audio_end_ms: number;
    }
  | {
      type: "conversation.item.create";
      item: {
        type: "function_call_output";
        call_id: string;
        output: string;
      };
    }
  | {
      type: "response.create";
    };

const TEST_WEBSOCKET_KEY = "__NORMA_TEST_WEBSOCKET__";
const OPENAI_REALTIME_BASE_URL = "wss://api.openai.com/v1/realtime";
const DEFAULT_MODEL = "gpt-realtime-2";
const DEFAULT_VOICE = "marin";
const DEFAULT_SAMPLE_RATE = 24000;
const DEFAULT_CHANNELS = 1;
const DEFAULT_AUDIO_FORMAT = "pcm16";
const unknownSessionId = "unknown";
const apiKey = process.env.OPENAI_API_KEY ?? "";
const readline = createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

let activeSessionId: string | undefined;
let activeSession: ActiveSession | undefined;
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

  if (commandType === "input_audio_chunk") {
    await handleInputAudioChunk(command);
    return;
  }

  if (commandType === "interrupt") {
    await handleInterrupt(command);
    return;
  }

  if (commandType === "tool_result") {
    await handleToolResult(command);
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

  if (apiKey.length === 0) {
    await writeSessionError(sessionId, "OPENAI_API_KEY is required for OpenAI realtime mode");
    return;
  }

  if (activeSession !== undefined) {
    await writeSessionError(sessionId, "an OpenAI realtime session is already active");
    return;
  }

  const model = modelFrom(command);
  activeSessionId = sessionId;
  const socket = createRealtimeSocket(model);
  const session: ActiveSession = {
    sessionId,
    socket,
    started: false,
    stoppingForUser: false,
    settledAfterClose: false,
    latestAssistantItemId: undefined,
    latestAssistantAudioEndMs: 0,
    openDeferred: undefined,
  };
  activeSession = session;
  attachSocketListeners(session);

  try {
    await waitForSocketOpen(session);
    await sendRealtimeEvent(session.socket, buildSessionUpdate(model));
    session.started = true;
    await writeEvent({
      type: "session_started",
      session_id: sessionId,
    });
  } catch (error) {
    clearActiveSession(session);
    await writeSessionError(sessionId, errorMessage(error));
  }
}

async function handleStopSession(command: JsonObject): Promise<void> {
  const sessionId = stringField(command, "session_id") ?? activeSessionId ?? unknownSessionId;
  if (activeSession !== undefined) {
    const session = activeSession;
    session.stoppingForUser = true;
    clearActiveSession(session);
    try {
      session.socket.close(1000, "user stopped");
    } catch {
      // Ignore close errors because the sidecar is exiting anyway.
    }
  }

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

async function handleInputAudioChunk(command: JsonObject): Promise<void> {
  const session = requireActiveSession(command, "input_audio_chunk requires an active session");
  if (session === undefined) {
    return;
  }

  const dataB64 = stringField(command, "data_b64");
  if (dataB64 === undefined) {
    await writeSessionError(session.sessionId, "input_audio_chunk is missing data_b64");
    return;
  }

  try {
    await sendRealtimeEvent(session.socket, {
      type: "input_audio_buffer.append",
      audio: dataB64,
    });
  } catch (error) {
    await writeSessionError(session.sessionId, errorMessage(error));
  }
}

async function handleInterrupt(command: JsonObject): Promise<void> {
  const session = requireActiveSession(command, "interrupt requires an active session");
  if (session === undefined) {
    return;
  }

  try {
    await sendRealtimeEvent(session.socket, {
      type: "response.cancel",
    });
    if (session.latestAssistantItemId !== undefined) {
      await sendRealtimeEvent(session.socket, {
        type: "conversation.item.truncate",
        item_id: session.latestAssistantItemId,
        content_index: 0,
        audio_end_ms: Math.max(0, Math.trunc(session.latestAssistantAudioEndMs)),
      });
    }
    await sendRealtimeEvent(session.socket, {
      type: "input_audio_buffer.clear",
    });
    await writeEvent({
      type: "assistant_audio_state",
      session_id: session.sessionId,
      speaking: false,
    });
  } catch (error) {
    await writeSessionError(session.sessionId, errorMessage(error));
  }
}

async function handleToolResult(command: JsonObject): Promise<void> {
  const session = requireActiveSession(command, "tool_result requires an active session");
  if (session === undefined) {
    return;
  }

  const toolCallId = stringField(command, "tool_call_id");
  if (toolCallId === undefined) {
    await writeSessionError(session.sessionId, "tool_result is missing tool_call_id");
    return;
  }

  const output = JSON.stringify(jsonField(command, "result") ?? {});
  try {
    await sendRealtimeEvent(session.socket, {
      type: "conversation.item.create",
      item: {
        type: "function_call_output",
        call_id: toolCallId,
        output,
      },
    });
    await sendRealtimeEvent(session.socket, {
      type: "response.create",
    });
  } catch (error) {
    await writeSessionError(session.sessionId, errorMessage(error));
  }
}

function requireActiveSession(command: JsonObject, message: string): ActiveSession | undefined {
  const sessionId = sessionIdFrom(command);
  if (activeSession === undefined || activeSession.sessionId !== sessionId) {
    void writeSessionError(sessionId, message);
    return undefined;
  }

  return activeSession;
}

function createRealtimeSocket(model: string): WebSocketLike {
  const url = new URL(OPENAI_REALTIME_BASE_URL);
  url.searchParams.set("model", model);
  const WebSocketCtor = websocketCtor();

  return new WebSocketCtor(url.toString(), {
    headers: {
      Authorization: `Bearer ${apiKey}`,
    },
  });
}

function websocketCtor(): WebSocketConstructor {
  const globals = globalThis as typeof globalThis & {
    [TEST_WEBSOCKET_KEY]?: WebSocketConstructor;
  };
  return globals[TEST_WEBSOCKET_KEY] ?? (WsWebSocket as unknown as WebSocketConstructor);
}

function attachSocketListeners(session: ActiveSession): void {
  session.socket.on("open", () => {
    session.openDeferred?.resolve();
    session.openDeferred = undefined;
  });

  session.socket.on("error", (error) => {
    if (!session.started) {
      session.openDeferred?.reject(new Error(errorMessage(error)));
      session.openDeferred = undefined;
      return;
    }

    void writeSessionError(session.sessionId, errorMessage(error));
  });

  session.socket.on("close", (code, reason) => {
    const closeReason = closeReasonMessage(code, reason);
    if (!session.started) {
      session.openDeferred?.reject(new Error(closeReason));
      session.openDeferred = undefined;
      clearActiveSession(session);
      return;
    }

    clearActiveSession(session);
    if (session.stoppingForUser || session.settledAfterClose) {
      return;
    }

    session.settledAfterClose = true;
    void writeEvent({
      type: "assistant_audio_state",
      session_id: session.sessionId,
      speaking: false,
    })
      .then(() =>
        writeEvent({
          type: "session_stopped",
          session_id: session.sessionId,
          reason: closeReason,
        }),
      )
      .catch(() => {
        process.exitCode = 1;
      });
  });

  session.socket.on("message", (data) => {
    queue = queue.then(() => handleSocketMessage(session, data)).catch(handleUnexpectedError);
  });
}

async function handleSocketMessage(session: ActiveSession, data: unknown): Promise<void> {
  const text = decodeSocketMessage(data);
  if (text === undefined) {
    await writeSessionError(session.sessionId, "OpenAI realtime sent a non-text WebSocket message");
    return;
  }

  let event: unknown;
  try {
    event = JSON.parse(text);
  } catch {
    await writeSessionError(session.sessionId, "OpenAI realtime sent malformed JSON");
    return;
  }

  if (!isJsonObject(event)) {
    await writeSessionError(session.sessionId, "OpenAI realtime event must be a JSON object");
    return;
  }

  const eventType = stringField(event, "type");
  if (eventType === undefined) {
    await writeSessionError(session.sessionId, "OpenAI realtime event is missing type");
    return;
  }

  updateAssistantPlaybackState(session, eventType, event);

  if (eventType === "error") {
    await writeSessionError(session.sessionId, upstreamErrorMessage(event));
    return;
  }

  if (eventType === "response.output_audio.delta" || eventType === "response.audio.delta") {
    const delta = stringField(event, "delta");
    if (delta !== undefined) {
      session.latestAssistantAudioEndMs += estimateAudioDeltaDurationMs(delta);
      await writeEvent({
        type: "assistant_audio_state",
        session_id: session.sessionId,
        speaking: true,
      });
      await writeEvent({
        type: "assistant_audio_chunk",
        session_id: session.sessionId,
        format: "pcm16",
        sample_rate: 24000,
        channels: 1,
        data_b64: delta,
      });
    }
    return;
  }

  if (
    eventType === "conversation.item.input_audio_transcription.delta" ||
    eventType === "conversation.item_input_audio_transcription.delta"
  ) {
    const textDelta = stringField(event, "delta") ?? stringField(event, "text");
    if (textDelta !== undefined) {
      await writeEvent({
        type: "transcript_delta",
        session_id: session.sessionId,
        text: textDelta,
      });
    }
    return;
  }

  if (
    eventType === "conversation.item.input_audio_transcription.completed" ||
    eventType === "conversation.item_input_audio_transcription.completed"
  ) {
    const transcript = stringField(event, "transcript") ?? stringField(event, "text");
    if (transcript !== undefined) {
      await writeEvent({
        type: "transcript_final",
        session_id: session.sessionId,
        text: transcript,
      });
    }
    return;
  }

  if (
    eventType === "response.output_audio_transcript.delta" ||
    eventType === "response.audio_transcript.delta" ||
    eventType === "response.output_text.delta"
  ) {
    const textDelta = stringField(event, "delta") ?? stringField(event, "text");
    if (textDelta !== undefined) {
      await writeEvent({
        type: "assistant_text_delta",
        session_id: session.sessionId,
        text: textDelta,
      });
    }
    return;
  }

  if (
    eventType === "response.output_audio.done" ||
    eventType === "response.audio.done" ||
    eventType === "response.done" ||
    eventType === "response.cancelled"
  ) {
    await writeEvent({
      type: "assistant_audio_state",
      session_id: session.sessionId,
      speaking: false,
    });
    return;
  }

  const toolCallEvent = toolCallEventFromRealtime(session.sessionId, eventType, event);
  if (toolCallEvent !== undefined) {
    await writeEvent(toolCallEvent);
  }
}

function decodeSocketMessage(data: unknown): string | undefined {
  if (typeof data === "string") {
    return data;
  }

  if (data instanceof ArrayBuffer) {
    return Buffer.from(data).toString("utf8");
  }

  if (Array.isArray(data)) {
    return Buffer.concat(data.map((item) => Buffer.from(item))).toString("utf8");
  }

  if (Buffer.isBuffer(data)) {
    return data.toString("utf8");
  }

  return undefined;
}

function waitForSocketOpen(session: ActiveSession): Promise<void> {
  return new Promise((resolve, reject) => {
    session.openDeferred = { resolve, reject };
  });
}

function buildSessionUpdate(model: string): OpenAiClientEvent {
  return {
    type: "session.update",
    session: {
      type: "realtime",
      model,
      output_modalities: ["audio", "text"],
      audio: {
        input: {
          format: {
            type: "audio/pcm",
            rate: DEFAULT_SAMPLE_RATE,
          },
          turn_detection: {
            type: "semantic_vad",
          },
        },
        output: {
          format: {
            type: "audio/pcm",
          },
          voice: DEFAULT_VOICE,
        },
      },
      tools: realtimeToolDefinitions(),
      tool_choice: "auto",
    },
  };
}

function realtimeToolDefinitions(): ToolDefinition[] {
  return [
    {
      type: "function",
      name: "enqueue_card_task",
      description: "Queue a card task request for later execution by the Rust app.",
      parameters: {
        type: "object",
        properties: {
          intent: {
            type: "string",
            description: "Natural-language task request from the user.",
          },
        },
        required: ["intent"],
      },
    },
    {
      type: "function",
      name: "cancel_card_task",
      description: "Cancel the latest queued or active card task if one exists.",
      parameters: {
        type: "object",
        properties: {
          scope: {
            type: "string",
            enum: ["latest"],
            description: "Which card task to cancel.",
          },
        },
        required: ["scope"],
      },
    },
  ];
}

function modelFrom(command: JsonObject): string {
  return (
    stringField(command, "model") ??
    stringField(configObject(command), "model") ??
    DEFAULT_MODEL
  );
}

function configObject(command: JsonObject): JsonObject {
  const config = command.config;
  return isJsonObject(config) ? config : {};
}

function offlineEchoEnabled(command: JsonObject): boolean {
  const config = configObject(command);
  if (config.offline_echo === true) {
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

function objectField(value: JsonObject, field: string): JsonObject | undefined {
  const fieldValue = value[field];
  return isJsonObject(fieldValue) ? fieldValue : undefined;
}

function jsonField(value: JsonObject, field: string): unknown {
  return field in value ? value[field] : undefined;
}

function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function toolCallEventFromRealtime(
  sessionId: string,
  eventType: string,
  event: JsonObject,
): Extract<NormaEvent, { type: "tool_call" }> | undefined {
  if (eventType === "response.output_item.done") {
    const item = objectField(event, "item");
    if (item === undefined || stringField(item, "type") !== "function_call") {
      return undefined;
    }
    return buildToolCallEvent(sessionId, item);
  }

  if (eventType === "response.function_call_arguments.done") {
    return buildToolCallEvent(sessionId, event);
  }

  return undefined;
}

function buildToolCallEvent(
  sessionId: string,
  source: JsonObject,
): Extract<NormaEvent, { type: "tool_call" }> | undefined {
  const toolCallId = stringField(source, "call_id") ?? stringField(source, "id");
  const name = stringField(source, "name");
  if (toolCallId === undefined || name === undefined) {
    return undefined;
  }

  return {
    type: "tool_call",
    session_id: sessionId,
    tool_call_id: toolCallId,
    name,
    arguments: jsonObjectFromUnknown(jsonField(source, "arguments")),
  };
}

function jsonObjectFromUnknown(value: unknown): JsonObject {
  if (isJsonObject(value)) {
    return value;
  }

  if (typeof value !== "string") {
    return {};
  }

  try {
    const parsed = JSON.parse(value);
    return isJsonObject(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function updateAssistantPlaybackState(
  session: ActiveSession,
  eventType: string,
  event: JsonObject,
): void {
  const assistantItemId = assistantItemIdFromEvent(eventType, event);
  if (assistantItemId === undefined) {
    return;
  }

  if (assistantItemId !== session.latestAssistantItemId) {
    session.latestAssistantItemId = assistantItemId;
    session.latestAssistantAudioEndMs = 0;
  }
}

function assistantItemIdFromEvent(eventType: string, event: JsonObject): string | undefined {
  if (eventType.startsWith("response.")) {
    return (
      stringField(event, "item_id") ??
      stringField(objectField(event, "item") ?? {}, "id") ??
      stringField(objectField(event, "output_item") ?? {}, "id")
    );
  }

  const item = objectField(event, "item");
  if (item !== undefined && stringField(item, "role") === "assistant") {
    return stringField(item, "id");
  }

  const outputItem = objectField(event, "output_item");
  if (outputItem !== undefined && stringField(outputItem, "role") === "assistant") {
    return stringField(outputItem, "id");
  }

  return undefined;
}

function estimateAudioDeltaDurationMs(delta: string): number {
  const pcmBytes = Buffer.from(delta, "base64").byteLength;
  const bytesPerMillisecond = (DEFAULT_SAMPLE_RATE * DEFAULT_CHANNELS * 2) / 1000;
  return Math.max(0, Math.floor(pcmBytes / bytesPerMillisecond));
}

async function sendRealtimeEvent(socket: WebSocketLike, event: OpenAiClientEvent): Promise<void> {
  const payload = JSON.stringify(event);
  return new Promise((resolve, reject) => {
    socket.send(payload, (error) => {
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
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

function clearActiveSession(session: ActiveSession): void {
  if (activeSession === session) {
    activeSession = undefined;
  }
  if (activeSessionId === session.sessionId) {
    activeSessionId = undefined;
  }
}

function closeReasonMessage(code: number, reason: Buffer): string {
  const text = reason.toString("utf8").trim();
  if (text.length > 0) {
    return sanitize(text);
  }
  return `OpenAI realtime WebSocket closed (${code})`;
}

function upstreamErrorMessage(event: JsonObject): string {
  const error = event.error;
  if (isJsonObject(error)) {
    return (
      stringField(error, "message") ??
      stringField(error, "type") ??
      "OpenAI realtime returned an error"
    );
  }

  return "OpenAI realtime returned an error";
}

function sanitize(message: string): string {
  let sanitized = message;
  if (apiKey.length > 0) {
    sanitized = sanitized.split(apiKey).join("[redacted]");
  }

  return sanitized.replace(/sk-[A-Za-z0-9_-]+/g, "sk-[redacted]");
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return sanitize(error.message);
  }

  return sanitize(String(error));
}

function handleUnexpectedError(error: unknown): void {
  const sessionId = activeSessionId ?? unknownSessionId;
  queue = writeSessionError(sessionId, errorMessage(error)).catch(() => {
    process.exitCode = 1;
  });
}
