import http from "node:http";

/**
 * Proveedor OpenAI-compatible simulado (sin cuota de modelo).
 * `steps` es una lista ordenada: cada llamada del CLI consume el siguiente paso;
 * el último se repite. Cada paso es `{ tool, args }` o `{ text }`.
 */
export function startMockProvider({ steps }) {
  if (!Array.isArray(steps) || steps.length === 0) {
    throw new Error("startMockProvider requiere al menos un paso");
  }
  const requests = [];
  const server = http.createServer((req, res) => {
    collect(req)
      .then((raw) => {
        if (!req.url?.endsWith("/chat/completions")) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end("{}");
          return;
        }
        const body = raw ? JSON.parse(raw) : {};
        // Las llamadas auxiliares del CLI (título de sesión) no llevan herramientas:
        // solo los turnos del agente principal consumen los pasos.
        const hasTools = Array.isArray(body.tools) && body.tools.length > 0;
        const used = requests.filter((item) => item.hasTools).length;
        const step = hasTools ? steps[Math.min(used, steps.length - 1)] : { text: "titulo" };
        requests.push({
          url: req.url,
          model: body.model,
          hasTools,
          messages: (body.messages ?? []).length,
          tools: (body.tools ?? []).map((tool) => tool.function?.name ?? tool.name),
          step: hasTools ? step.tool ?? "text" : "titulo",
        });
        const payload = { id: `chatcmpl_${requests.length}`, model: body.model ?? "mock", step };
        if (body.stream === false) respondJson(res, payload);
        else respondStream(res, payload);
      })
      .catch(() => {
        res.writeHead(500, { "content-type": "application/json" });
        res.end("{}");
      });
  });

  return listen(server).then((port) => ({
    url: `http://127.0.0.1:${port}/v1`,
    requests,
    close: () => new Promise((resolve) => server.close(resolve)),
  }));
}

function collect(req) {
  return new Promise((resolve) => {
    let raw = "";
    req.on("data", (chunk) => (raw += chunk));
    req.on("end", () => resolve(raw));
  });
}

function messageOf(step) {
  if (step.tool) {
    return {
      role: "assistant",
      content: null,
      tool_calls: [
        {
          id: "call_uac_1",
          type: "function",
          function: { name: step.tool, arguments: JSON.stringify(step.args ?? {}) },
        },
      ],
    };
  }
  return { role: "assistant", content: step.text ?? "fin" };
}

function respondJson(res, { id, model, step }) {
  const toolCall = Boolean(step.tool);
  const body = {
    id,
    object: "chat.completion",
    created: 0,
    model,
    choices: [{ index: 0, message: messageOf(step), finish_reason: toolCall ? "tool_calls" : "stop" }],
    usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
  };
  res.writeHead(200, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}

function respondStream(res, { id, model, step }) {
  const message = messageOf(step);
  const delta = message.tool_calls
    ? { role: "assistant", content: null, tool_calls: message.tool_calls }
    : { role: "assistant", content: message.content };
  const chunk = (payload) => `data: ${JSON.stringify(payload)}\n\n`;
  const base = { id, object: "chat.completion.chunk", created: 0, model };
  res.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  res.write(chunk({ ...base, choices: [{ index: 0, delta, finish_reason: null }] }));
  res.write(
    chunk({
      ...base,
      choices: [{ index: 0, delta: {}, finish_reason: message.tool_calls ? "tool_calls" : "stop" }],
      usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
    }),
  );
  res.write("data: [DONE]\n\n");
  res.end();
}

function listen(server) {
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => resolve(server.address().port));
  });
}
