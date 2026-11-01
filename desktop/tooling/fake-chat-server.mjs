// Deterministic chat host for UI evidence: no provider, no real data.
//   node tooling/fake-chat-server.mjs [port]
//   GET /__mode?m=ok|busy|forbidden|seat|invalid
// The signature is accepted without verifying it; this server is only for
// screenshots and local UI smoke tests.

import { createServer } from "node:http";

const port = Number(process.argv[2] || 8791);
let mode = "ok";
let revision = 1;
const conversationId = "cnv_01DEMODESKTOP8000000000000";

const conversation = {
  id: conversationId,
  title: "Plan del proyecto Demo",
  pinned: true,
  archived: false,
  last_message_at: "2026-10-22T18:20:00.000Z",
  last_seq: 4,
  last_read_seq: 2,
  unread: 1,
  created_at: "2026-10-22T18:00:00.000Z",
  updated_at: "2026-10-22T18:20:00.000Z",
};

const messages = [
  {
    id: "msg_demo_1",
    conversation_id: conversationId,
    seq: 1,
    author_kind: "user",
    body_md: "Crea el proyecto Demo y pide a Ernesto un plan de 3 hitos",
    state: "complete",
    created_at: "2026-10-22T18:01:00.000Z",
  },
  {
    id: "msg_demo_2",
    conversation_id: conversationId,
    seq: 2,
    author_kind: "agent",
    agent_id: "ag_alfred",
    body_md:
      "Hecho. El proyecto Demo ya existe y Ernesto es el lead. Te aviso cuando tenga el plan.",
    state: "complete",
    created_at: "2026-10-22T18:02:00.000Z",
  },
  {
    id: "msg_demo_3",
    conversation_id: conversationId,
    seq: 3,
    author_kind: "user",
    body_md: "Resume el estado del proyecto Demo.",
    state: "complete",
    created_at: "2026-10-22T18:19:00.000Z",
  },
  {
    id: "msg_demo_4",
    conversation_id: conversationId,
    seq: 4,
    author_kind: "agent",
    agent_id: "ag_alfred",
    body_md:
      "Tres hitos: contrato y datos, timeline y búsqueda, y por último el empaquetado. El riesgo principal es la migración de chat.db.",
    state: "complete",
    created_at: "2026-10-22T18:20:00.000Z",
  },
];
const timeline = [
  { seq: 1, kind: "message", message_id: "msg_demo_1", author_kind: "user", preview: messages[0].body_md, state: "complete", created_at: messages[0].created_at },
  { seq: 2, kind: "message", message_id: "msg_demo_2", author_kind: "agent", preview: messages[1].body_md, state: "complete", created_at: messages[1].created_at },
  { seq: 3, kind: "message", message_id: "msg_demo_3", author_kind: "user", preview: messages[2].body_md, state: "complete", created_at: messages[2].created_at },
  { seq: 4, kind: "message", message_id: "msg_demo_4", author_kind: "agent", preview: messages[3].body_md, state: "complete", created_at: messages[3].created_at },
  {
    seq: 5,
    kind: "delegation_reminder",
    preview: "Entrada de un host más nuevo",
    created_at: messages[3].created_at,
  },
];

function json(response, status, body, headers = {}) {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(payload),
    ...headers,
  });
  response.end(payload);
}

// One mail batch waiting for the operator in the inbox. Its rules for `exclude` are
// the host's: ids of the batch only, never all of them, and only with approve.
const approvals = [
  {
    id: "apr_01DEMOMAILMOVE0000000000",
    conversation_id: null,
    seat_id: "ag_ernesto",
    agent_label: "Ernesto",
    kind: "tool",
    tool: "mail.move",
    summary: "Mover 3 correos de la cuenta de trabajo",
    risk: "medium",
    action_digest: "sha256:4f1a9c0d2b7e",
    status: "pending",
    scope: "once",
    expires_at: new Date(Date.now() + 10 * 60_000).toISOString(),
    level: "user",
    details: {
      account: "trabajo",
      items: [
        { message_id: "m1", dest: "junkemail", reason: "Publicidad sin relación con el proyecto", from_address: "ofertas@tienda.invalid", subject: "Última oportunidad: 70 % de descuento", level: "bulk", excluded: false },
        { message_id: "m2", dest: "deleteditems", reason: "Boletín que nadie abre", from_address: "news@boletin.invalid", subject: "Resumen semanal", level: "bulk", excluded: false },
        { message_id: "m3", dest: "inbox", reason: "Factura de un cliente", from_address: "bea@ejemplo.invalid", subject: "Factura de octubre", level: "bulk", excluded: false },
      ],
    },
    audit: [],
  },
];

const pendingCount = () => approvals.filter((row) => row.status === "pending").length;

async function readJson(request) {
  let raw = "";
  for await (const chunk of request) raw += chunk;
  try {
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

// The host's checks on `exclude`, in its order. Null when the list is fine.
function exclusionError(row, body) {
  const exclude = body.exclude || [];
  if (!Array.isArray(exclude) || exclude.length > 20 || exclude.some((id) => typeof id !== "string" || !id || id.length > 512)) {
    return { code: "invalid_body", field: "exclude" };
  }
  if (!exclude.length) return null;
  if (body.decision !== "approve" || row.tool !== "mail.move") return { code: "invalid_exclusion" };
  const batch = row.details.items.map((item) => item.message_id);
  if (exclude.some((id) => !batch.includes(id))) return { code: "unknown_message" };
  if (batch.every((id) => exclude.includes(id))) return { code: "invalid_exclusion" };
  return null;
}

function decideApproval(response, row, body) {
  if (row.status !== "pending") {
    // The same client id gets the same answer back, as on the host.
    if (row.client_id === body.client_id) json(response, 200, { ok: true, approval: row });
    else json(response, 409, { detail: { code: "approval_already_decided" } });
    return;
  }
  if (!["approve", "deny"].includes(body.decision)) {
    json(response, 422, { detail: { code: "invalid_body", field: "decision" } });
    return;
  }
  const error = exclusionError(row, body);
  if (error) {
    json(response, 422, { detail: error });
    return;
  }
  if (body.decision === "approve") {
    const left = new Set(body.exclude || []);
    for (const item of row.details.items) item.excluded = left.has(item.message_id);
  }
  row.status = body.decision === "approve" ? "approved" : "denied";
  row.client_id = body.client_id;
  row.decided_at = new Date().toISOString();
  json(response, 200, { ok: true, approval: row });
}

const server = createServer(async (request, response) => {
  const url = new URL(request.url, `http://127.0.0.1:${port}`);
  if (url.pathname === "/__mode") {
    mode = url.searchParams.get("m") || "ok";
    json(response, 200, { mode });
    return;
  }
  if (url.pathname === "/v1/health") {
    json(response, 200, { ok: true, devices: 1, revision, streams: 1 });
    return;
  }
  if (url.pathname === "/v1/state") {
    json(response, 200, {
      revision,
      herdr_version: "0.9.1",
      counts: { idle: 1 },
      agents: [
        {
          pane_id: "w1:p1",
          project: "Demo",
          agent: "claude",
          status: "working",
          attention: 1,
          title: "Plan de 3 hitos",
        },
      ],
      chat: { revision, unread: 1, pending_approvals: pendingCount(), active_delegations: 0 },
    });
    return;
  }
  if (url.pathname === "/v1/events") {
    response.writeHead(200, {
      "content-type": "text/event-stream",
      "cache-control": "no-store",
    });
    response.write(
      `event: state\ndata: ${JSON.stringify({
        event: "state",
        revision,
        state: {
          revision,
          herdr_version: "0.9.1",
          counts: { idle: 1 },
          agents: [],
          chat: { revision, unread: 1, pending_approvals: pendingCount(), active_delegations: 0 },
        },
      })}\n\n`,
    );
    return;
  }
  if (url.pathname === "/v1/chat/bootstrap") {
    json(response, 200, {
      ok: true,
      company: { id: "co_demo", name: "Demo" },
      projects: [
        {
          id: "prj_demo",
          name: "Demo",
          root_label: "demo",
          status: "active",
          lead: { agent_id: "ag_ernesto", name: "Ernesto", status: "working", uac_state: "safe" },
        },
      ],
      alfred: { agent_id: "ag_alfred", name: "Alfred", status: "idle", uac_state: "safe" },
      unread: { conversations: 1, messages: 1, approvals: pendingCount() },
      chat: { revision, unread: 1, pending_approvals: pendingCount(), active_delegations: 1 },
    });
    return;
  }
  if (url.pathname === "/v1/chat/conversations" && request.method === "GET") {
    const archived = url.searchParams.get("archived") === "true";
    const q = (url.searchParams.get("q") || "").toLowerCase();
    const rows = archived ? [] : q ? [conversation].filter((row) => row.title.toLowerCase().includes(q)) : [conversation];
    json(response, 200, { ok: true, conversations: rows });
    return;
  }
  if (url.pathname === "/v1/chat/approvals" && request.method === "GET") {
    // A conversation-less approval shows only in the inbox, as on the host.
    const rows = url.searchParams.has("conversation_id")
      ? []
      : approvals.filter((row) => row.status === (url.searchParams.get("status") || "pending"));
    json(response, 200, { ok: true, approvals: rows });
    return;
  }
  const decided = /^\/v1\/chat\/approvals\/([^/]+)\/decision$/.exec(url.pathname);
  if (decided && request.method === "POST") {
    const row = approvals.find((item) => item.id === decided[1]);
    if (!row) {
      json(response, 404, { detail: "not_found" });
      return;
    }
    decideApproval(response, row, await readJson(request));
    return;
  }
  if (url.pathname === "/v1/chat/sections") {
    json(response, 200, { ok: true, sections: [] });
    return;
  }
  if (url.pathname === `/v1/chat/conversations/${conversationId}/messages` && request.method === "GET") {
    json(response, 200, { ok: true, messages, next_before_seq: null });
    return;
  }
  if (url.pathname === `/v1/chat/conversations/${conversationId}/timeline`) {
    json(response, 200, { ok: true, entries: timeline, conversation_id: conversationId });
    return;
  }
  if (url.pathname === `/v1/chat/conversations/${conversationId}`) {
    json(response, 200, {
      ok: true,
      conversation,
      participants: [{ agent_id: "ag_alfred", name: "Alfred", status: "idle", uac_state: "safe" }],
    });
    return;
  }
  if (url.pathname === `/v1/chat/conversations/${conversationId}/messages` && request.method === "POST") {
    if (mode === "forbidden") {
      json(response, 403, { detail: "forbidden" });
      return;
    }
    if (mode === "busy") {
      json(response, 429, { detail: "agent_busy" }, { "retry-after": "18" });
      return;
    }
    if (mode === "seat") {
      json(response, 409, { detail: "seat_unavailable" });
      return;
    }
    if (mode === "invalid") {
      json(response, 422, {
        detail: { code: "invalid_body", errors: [{ loc: ["text"], msg: "too long" }] },
      });
      return;
    }
    const seq = messages.length + 1;
    json(
      response,
      202,
      {
        ok: true,
        message: {
          id: `msg_demo_${seq}`,
          conversation_id: conversationId,
          seq,
          author_kind: "user",
          body_md: "Mensaje de prueba",
          state: "complete",
          created_at: new Date().toISOString(),
        },
        turn: { id: "run_demo", state: "queued", agent_id: "ag_alfred" },
      },
    );
    return;
  }
  if (url.pathname.endsWith("/read")) {
    json(response, 200, { ok: true, conversation_id: conversationId, last_read_seq: 4 });
    return;
  }
  json(response, 404, { detail: "not_found" });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`fake chat host on http://127.0.0.1:${port}`);
});

