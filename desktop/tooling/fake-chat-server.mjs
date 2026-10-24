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

const server = createServer((request, response) => {
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
      chat: { revision, unread: 1, pending_approvals: 0, active_delegations: 0 },
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
          chat: { revision, unread: 1, pending_approvals: 0, active_delegations: 0 },
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
      unread: { conversations: 1, messages: 1, approvals: 0 },
      chat: { revision, unread: 1, pending_approvals: 0, active_delegations: 1 },
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

