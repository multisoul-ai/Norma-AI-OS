#!/usr/bin/env node

const REQUEST_TIMEOUT_MS = 30000;

try {
  const input = JSON.parse(await readStdin());
  const apiKey = process.env.OPENAI_API_KEY;
  if (typeof apiKey !== "string" || apiKey.trim() === "") {
    throw new Error("OPENAI_API_KEY is required");
  }
  const model = stringField(input, "model");
  const request = objectField(input, "request");
  const plan = await requestVoicePlan(apiKey, model, request);
  process.stdout.write(plan);
} catch (error) {
  process.stderr.write(errorMessage(error));
  process.exit(1);
}

async function requestVoicePlan(apiKey, model, request) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  try {
    const response = await fetch("https://api.openai.com/v1/responses", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiKey}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        model,
        input: [
          {
            role: "system",
            content: [
              {
                type: "input_text",
                text: systemPrompt(),
              },
            ],
          },
          {
            role: "user",
            content: [
              {
                type: "input_text",
                text: JSON.stringify(request),
              },
            ],
          },
        ],
        text: { format: { type: "json_object" } },
      }),
      signal: controller.signal,
    });
    const body = await response.text();
    if (!response.ok) {
      throw new Error(`OpenAI card planner failed with HTTP ${response.status}: ${body}`);
    }
    const data = JSON.parse(body);
    const text = extractOutputText(data);
    const json = extractJsonObject(text);
    JSON.parse(json);
    return json;
  } finally {
    clearTimeout(timeout);
  }
}

function systemPrompt() {
  return [
    "You are Norma OS's realtime card task planner.",
    "Return exactly one JSON object and no markdown.",
    "The user message is a VoicePlanningRequest JSON object with transcript, cards, focused_node_id, selected_node_ids, and max_steps.",
    'Return a VoicePlan: {"commands":[],"confidence":0.9,"risk":"low","requires_confirmation":false,"reason":"short reason"}.',
    "Allowed commands:",
    '{"type":"create","kind":{"kind":"terminal"}}',
    '{"type":"create","kind":{"kind":"agent","profile":"Codex"}}',
    '{"type":"create","kind":{"kind":"agent","profile":"Claude"}}',
    '{"type":"create","kind":{"kind":"agent","profile":"OpenCode"}}',
    '{"type":"focus","target":{"type":"node_id","node_id":1}}',
    '{"type":"send_text","target":{"type":"previous_step","step":0},"text":"hello","submit":false}',
    "Allowed targets: node_id, alias, previous_step, focused, selected.",
    "Allowed risk values: low, medium, high.",
    "Do not invent unsupported commands.",
  ].join("\n");
}

function extractOutputText(data) {
  if (typeof data.output_text === "string") {
    return data.output_text;
  }
  const chunks = [];
  for (const item of Array.isArray(data.output) ? data.output : []) {
    for (const content of Array.isArray(item.content) ? item.content : []) {
      if (typeof content.text === "string") {
        chunks.push(content.text);
      }
    }
  }
  if (chunks.length === 0) {
    throw new Error("OpenAI card planner response did not contain output_text");
  }
  return chunks.join("");
}

function extractJsonObject(text) {
  const trimmed = text.trim();
  if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
    return trimmed;
  }
  const start = trimmed.indexOf("{");
  const end = trimmed.lastIndexOf("}");
  if (start < 0 || end <= start) {
    throw new Error("OpenAI card planner output did not contain a JSON object");
  }
  return trimmed.slice(start, end + 1);
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

function stringField(value, field) {
  if (typeof value?.[field] !== "string" || value[field].trim() === "") {
    throw new Error(`${field} is required`);
  }
  return value[field];
}

function objectField(value, field) {
  if (typeof value?.[field] !== "object" || value[field] === null || Array.isArray(value[field])) {
    throw new Error(`${field} object is required`);
  }
  return value[field];
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}
