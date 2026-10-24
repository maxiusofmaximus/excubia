import test from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import path from "node:path";
import plugin, { guard } from "../index.js";
import { actionDigest, redact } from "../src/digest.js";

const cwd = path.resolve("D:\\proyectos\\demo");
const event = { tool: "bash", input: { command: "echo hola" }, sessionID: "ses_test", agent: "build", id: "call_1" };

function listen(handler) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      let raw = "";
      req.on("data", (chunk) => (raw += chunk));
      req.on("end", () => {
        const body = raw ? JSON.parse(raw) : undefined;
        const result = handler(req, body);
        res.writeHead(result.status ?? 200, { "content-type": "application/json" });
        res.end(JSON.stringify(result.body ?? {}));
      });
    });
    server.listen(0, "127.0.0.1", () => resolve({ server, base: `http://127.0.0.1:${server.address().port}` }));
  });
}

function close(server) {
  return new Promise((resolve) => server.close(resolve));
}

function envFor(base, extra = {}) {
  return {
    HN_HOOK_URL: base,
    HN_HOOK_TOKEN: "token-de-prueba",
    HN_PANE_ID: "w1:p9",
    HN_UAC_TTL_SECONDS: "1",
    HN_UAC_POLL_MS: "25",
    ...extra,
  };
}

async function captureHook() {
  const hooks = {};
  const ctx = {
    location: { directory: cwd },
    tool: {
      hook: async (name, callback) => {
        hooks[name] = callback;
        return { dispose: async () => {} };
      },
    },
  };
  const cleanup = await plugin.setup(ctx);
  return { hook: hooks["execute.before"], cleanup };
}

async function withEnv(env, fn) {
  const saved = {};
  for (const [key, value] of Object.entries(env)) {
    saved[key] = process.env[key];
    process.env[key] = value;
  }
  try {
    return await fn();
  } finally {
    for (const key of Object.keys(env)) {
      if (saved[key] === undefined) delete process.env[key];
      else process.env[key] = saved[key];
    }
  }
}

test("setup registra execute.before y el cleanup no falla", async () => {
  const { hook, cleanup } = await captureHook();
  assert.equal(typeof hook, "function");
  await cleanup();
});

test("aprueba, consume y deja ejecutar", async () => {
  const calls = [];
  const { server, base } = await listen((req) => {
    calls.push(`${req.method} ${req.url}`);
    if (req.method === "POST" && req.url === "/v1/chat/approvals") {
      return { status: 201, body: { approval_id: "apr_1", status: "pending", expires_at: new Date(Date.now() + 5000).toISOString() } };
    }
    if (req.method === "GET") return { body: { approval_id: "apr_1", status: "approved", expires_at: new Date(Date.now() + 5000).toISOString() } };
    if (req.method === "POST" && req.url.endsWith("/consume")) return { body: { approval_id: "apr_1", status: "consumed" } };
    return { status: 404, body: {} };
  });
  try {
    const { hook } = await captureHook();
    await withEnv(envFor(base), () => hook(event));
    assert.deepEqual(calls, ["POST /v1/chat/approvals", "GET /v1/chat/approvals/apr_1", "POST /v1/chat/approvals/apr_1/consume"]);
  } finally {
    await close(server);
  }
});

test("denegación del operador bloquea", async () => {
  const { server, base } = await listen((req) => {
    if (req.method === "POST" && req.url === "/v1/chat/approvals") {
      return { status: 201, body: { approval_id: "apr_2", status: "pending", expires_at: new Date(Date.now() + 5000).toISOString() } };
    }
    return { body: { approval_id: "apr_2", status: "denied" } };
  });
  try {
    await assert.rejects(() => guard(event, { cwd, env: envFor(base) }), /denegada por el operador/);
  } finally {
    await close(server);
  }
});

test("timeout de TTL bloquea", async () => {
  const { server, base } = await listen((req) => {
    if (req.method === "POST" && req.url === "/v1/chat/approvals") {
      return { status: 201, body: { approval_id: "apr_3", status: "pending", expires_at: new Date(Date.now() + 5000).toISOString() } };
    }
    return { body: { approval_id: "apr_3", status: "pending" } };
  });
  try {
    await assert.rejects(() => guard(event, { cwd, env: envFor(base) }), /expirada tras 1 s/);
  } finally {
    await close(server);
  }
});

test("host caído bloquea", async () => {
  const env = envFor("http://127.0.0.1:1");
  await assert.rejects(() => guard(event, { cwd, env }), /no responde/);
});

test("consume fallido bloquea", async () => {
  const { server, base } = await listen((req) => {
    if (req.method === "POST" && req.url === "/v1/chat/approvals") {
      return { status: 201, body: { approval_id: "apr_4", status: "pending", expires_at: new Date(Date.now() + 5000).toISOString() } };
    }
    if (req.method === "GET") return { body: { approval_id: "apr_4", status: "approved" } };
    return { status: 409, body: { code: "approval_expired" } };
  });
  try {
    await assert.rejects(() => guard(event, { cwd, env: envFor(base) }), /HTTP 409/);
  } finally {
    await close(server);
  }
});

test("sin configuración bloquea", async () => {
  await assert.rejects(() => guard(event, { cwd, env: {} }), /no configurada/);
});

test("HN_HOOK_URL no loopback bloquea", async () => {
  const env = { HN_HOOK_URL: "http://example.com", HN_HOOK_TOKEN: "x" };
  await assert.rejects(() => guard(event, { cwd, env }), /loopback/);
});

test("el digest es estable y redacta secretos", () => {
  const a = actionDigest({ kind: "command", tool: "bash", args: { b: 2, a: 1 }, cwd, paths: ["C:\\x", "C:\\y"] });
  const b = actionDigest({ kind: "command", tool: "bash", args: { a: 1, b: 2 }, cwd, paths: ["C:\\y", "C:\\x"] });
  assert.equal(a, b);
  assert.match(a, /^sha256:[0-9a-f]{64}$/);
  assert.deepEqual(redact({ token: "secreto", command: "echo hola" }), { token: "[redactado]", command: "echo hola" });
  assert.deepEqual(redact({ command: "cli --token abc123" }), { command: "cli --token [redactado]" });
  assert.deepEqual(redact({ command: "curl -H 'Authorization: Bearer abc.def'" }), { command: "curl -H 'Authorization: [redactado]'" });
});

async function v1Hook(directory = cwd) {
  const hooks = await plugin.server({ directory });
  return hooks["tool.execute.before"];
}

test("el adaptador V1 expone tool.execute.before", async () => {
  assert.equal(typeof (await v1Hook()), "function");
});

test("V1 usa output.args y el directory del servidor al pedir aprobación", async () => {
  const calls = [];
  let posted;
  const { server, base } = await listen((req, body) => {
    calls.push(`${req.method} ${req.url}`);
    if (req.method === "POST" && req.url === "/v1/chat/approvals") {
      posted = body;
      return { status: 201, body: { approval_id: "apr_v1", status: "pending", expires_at: new Date(Date.now() + 5000).toISOString() } };
    }
    if (req.method === "GET") return { body: { approval_id: "apr_v1", status: "approved" } };
    return { body: { approval_id: "apr_v1", status: "consumed" } };
  });
  try {
    const hook = await v1Hook();
    await withEnv(envFor(base), () =>
      hook({ tool: "bash", callID: "call_v1", sessionID: "ses_v1" }, { args: { command: "echo desde-v1" } }),
    );
    assert.deepEqual(calls, ["POST /v1/chat/approvals", "GET /v1/chat/approvals/apr_v1", "POST /v1/chat/approvals/apr_v1/consume"]);
    assert.equal(posted.cwd, cwd);
    assert.equal(posted.session_id, "ses_v1");
    assert.match(posted.summary, /echo desde-v1/);
    assert.match(posted.action_digest, /^sha256:[0-9a-f]{64}$/);
  } finally {
    await close(server);
  }
});

test("V1 bloquea si el operador deniega", async () => {
  const { server, base } = await listen((req) => {
    if (req.method === "POST" && req.url === "/v1/chat/approvals") {
      return { status: 201, body: { approval_id: "apr_v1d", status: "pending", expires_at: new Date(Date.now() + 5000).toISOString() } };
    }
    return { body: { approval_id: "apr_v1d", status: "denied" } };
  });
  try {
    const hook = await v1Hook();
    await assert.rejects(
      () => withEnv(envFor(base), () => hook({ tool: "bash", callID: "call_v1d" }, { args: { command: "echo hola" } })),
      /denegada por el operador/,
    );
  } finally {
    await close(server);
  }
});

test("V1 sin configuración bloquea", async () => {
  const hook = await v1Hook();
  await withEnv({ HN_HOOK_URL: "", HN_HOOK_TOKEN: "" }, () =>
    assert.rejects(
      () => hook({ tool: "bash", callID: "call_v1s" }, { args: { command: "echo hola" } }),
      /no configurada/,
    ),
  );
});

test("V1 no pide aprobación para lectura dentro del cwd", async () => {
  const hook = await v1Hook();
  await withEnv(envFor("http://127.0.0.1:1"), () =>
    hook({ tool: "read", callID: "call_v1r" }, { args: { filePath: path.join(cwd, "inside.txt") } }),
  );
});
