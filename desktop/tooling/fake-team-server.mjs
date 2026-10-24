// Deterministic delegation host for UI evidence: cards in every state, typed
// refusals and a project team. No provider and no real data.
//   node tooling/fake-team-server.mjs [port]

import { createServer } from "node:http";

const port = Number(process.argv[2] || 8793);
const now = "2026-10-22T19:00:00.000Z";
const demo = "cnv_01TEAMDEMO00000000000000";
const errors = "cnv_01TEAMERRORS000000000000";
let revision = 10;

const cards = new Map();
const details = new Map();

function addCard(id, conversation, seq, title, summary, status, pane, attention = 0, agent = "ag_team") {
  const card = {
    id: `act_${id}`,
    seq,
    kind: "card",
    card_id: `act_${id}`,
    delegation_id: id,
    ref_id: id,
    title,
    summary,
    status,
    agent_id: agent,
    pane_id: pane,
    attention,
    links: { agent: `/v1/agents/${pane}`, delegation: `/v1/chat/delegations/${id}` },
    created_at: now,
  };
  cards.set(id, { card, conversation });
  details.set(id, {
    delegation: {
      id,
      conversation_id: conversation,
      project_id: "prj_demo",
      parent_id: null,
      from_agent_id: "ag_alfred",
      from_name: "Alfred",
      to_agent_id: agent,
      to_name: title.includes("Bob") ? "Bob" : title.includes("Ines") ? "Ines" : "Ernesto",
      task: summary,
      state: status,
      depth: 1,
      turn_key: "run_team",
      pane_id: pane,
      result_message_id: status === "done" ? `msg_${id}` : null,
      reason: ["failed", "cancelled", "interrupted"].includes(status) ? summary : null,
      created_at: now,
      updated_at: now,
      finished_at: ["done", "failed", "cancelled", "interrupted"].includes(status) ? now : null,
    },
    children: [],
    card,
    result_message:
      status === "done"
        ? {
            id: `msg_${id}`,
            conversation_id: conversation,
            seq: seq + 1,
            author_kind: "agent",
            body_md: "Migración aplicada: 3 tests OK, sin cambios en producción.",
            state: "complete",
            created_at: now,
          }
        : null,
    synthesis: status === "done" ? { state: "queued", run_id: "run_synthesis" } : null,
  });
  return card;
}

const parent = "dlg_01RUNALFRED000000000000";
const waiting = "dlg_01WAITBOB0000000000000";
const done = "dlg_01DONEBOB0000000000000";
const failed = "dlg_01FAILINES000000000000";
const cancelled = "dlg_01CANCEL00000000000000";
const interrupted = "dlg_01INTERRUPT00000000000";

addCard(parent, demo, 2, "Alfred pidió a Ernesto", "Plan del proyecto Demo · 3 hitos y dependencias", "running", "w1:p3");
addCard(waiting, demo, 5, "Ernesto asignó a Bob", "Migración de chat.db · ejecuta cargo test", "waiting", "w1:p9", 3);
addCard(done, demo, 7, "Bob terminó", "Migración aplicada · 3 tests OK", "done", "w1:p9");
addCard(failed, demo, 9, "Ines no terminó", "delegation_timeout", "failed", "w1:p11");
addCard(cancelled, demo, 11, "Ernesto cancelado", "Publicar el build del viernes", "cancelled", "w1:p3");
addCard(interrupted, demo, 13, "Bob interrumpido", "host_restart", "interrupted", "w1:p9");

details.get(parent).children = [
  { id: waiting, depth: 2 },
  { id: done, depth: 2 },
];

const errorCodes = [
  ["dlg_01ERRDEPTH00000000000000", "depth_exceeded"],
  ["dlg_01ERRPANE000000000000000", "pane_limit"],
  ["dlg_01ERRCYCLE0000000000000", "cycle_detected"],
  ["dlg_01ERRTURN00000000000000", "turn_budget"],
  ["dlg_01ERRHOUR00000000000000", "hour_budget"],
  ["dlg_01ERRSEAT00000000000000", "seat_unavailable"],
];
errorCodes.forEach(([id, code], index) => {
  addCard(id, errors, index * 2 + 2, "Ernesto no terminó", code, "failed", "w1:p3");
});

const conversations = [
  {
    id: demo,
    title: "Equipo Demo",
    pinned: true,
    archived: false,
    last_message_at: now,
    last_seq: 13,
    last_read_seq: 1,
    unread: 1,
    created_at: now,
    updated_at: now,
  },
  {
    id: errors,
    title: "Errores del equipo",
    pinned: false,
    archived: false,
    last_message_at: now,
    last_seq: 12,
    last_read_seq: 12,
    unread: 0,
    created_at: now,
    updated_at: now,
  },
];

const messages = new Map([
  [
    demo,
    [
      {
        id: "msg_demo_1",
        conversation_id: demo,
        seq: 1,
        author_kind: "user",
        body_md: "Crea el proyecto Demo y pide a Ernesto un plan de 3 hitos.",
        state: "complete",
        created_at: now,
      },
      {
        id: "msg_demo_2",
        conversation_id: demo,
        seq: 3,
        author_kind: "agent",
        author_agent_id: "ag_alfred",
        body_md: "Hecho. El proyecto Demo ya existe y Ernesto es el lead.",
        state: "complete",
        created_at: now,
      },
      {
        id: "msg_demo_3",
        conversation_id: demo,
        seq: 6,
        author_kind: "user",
        body_md: "¿Puede Bob ejecutar la suite antes de cerrar?",
        state: "complete",
        created_at: now,
      },
    ],
  ],
  [
    errors,
    [
      {
        id: "msg_errors_1",
        conversation_id: errors,
        seq: 1,
        author_kind: "user",
        body_md: "Lanza el resto del plan.",
        state: "complete",
        created_at: now,
      },
    ],
  ],
]);

const stateAgents = [
  ["w1:p3", "Demo", "working", "Ernesto — lead", "Ernesto"],
  ["w1:p9", "Demo", "blocked", "Bob — especialista", "Bob"],
  ["w1:p11", "Demo", "idle", "Ines — especialista", "Ines"],
  ["w1:p20", "herdr-notify", "idle", "Hugo — lead", "Hugo"],
  ["w1:p21", "herdr-notify", "working", "Sami — especialista", "Sami"],
].map(([pane, project, status, title, agent]) => ({
  pane_id: pane,
  project,
  agent,
  status,
  attention: status === "blocked" ? 3 : 0,
  title,
  cwd: "",
  focused: false,
  can_resume: false,
  restored: false,
}));

function json(response, status, body) {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    "content-type": "application/json",
    "content-length": Buffer.byteLength(payload),
  });
  response.end(payload);
}

function timeline(conversation) {
  const rows = [];
  for (const { card, conversation: owner } of cards.values()) {
    if (owner === conversation) rows.push(card);
  }
  return rows.sort((a, b) => a.seq - b.seq);
}

function refreshCard(id, status, title, summary) {
  const record = cards.get(id);
  if (!record) return null;
  record.card.status = status;
  record.card.title = title;
  record.card.summary = summary;
  record.card.attention = 0;
  const detail = details.get(id);
  detail.delegation.state = status;
  detail.card = record.card;
  detail.delegation.reason = ["failed", "cancelled", "interrupted"].includes(status) ? summary : null;
  detail.delegation.finished_at = now;
  return detail;
}

const server = createServer((request, response) => {
  const url = new URL(request.url, `http://127.0.0.1:${port}`);
  const path = url.pathname;

  if (path === "/v1/health") return json(response, 200, { ok: true, devices: 1, revision, streams: 1 });
  if (path === "/v1/state") {
    const agents = stateAgents;
    return json(response, 200, {
      revision,
      herdr_version: "0.9.1",
      counts: { working: 2, blocked: 1, idle: 2 },
      agents,
      chat: { revision, unread: 1, pending_approvals: 0, active_delegations: 3 },
    });
  }
  if (path === "/v1/events") {
    response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-store" });
    response.write(
      `event: state\ndata: ${JSON.stringify({ event: "state", revision, state: { revision, herdr_version: "0.9.1", counts: {}, agents: stateAgents, chat: { revision, unread: 1, pending_approvals: 0, active_delegations: 3 } } })}\n\n`,
    );
    return;
  }
  if (path === "/v1/chat/bootstrap") {
    return json(response, 200, {
      ok: true,
      company: { id: "co_demo", name: "Demo" },
      projects: [
        {
          id: "prj_demo",
          name: "Demo",
          root_label: "demo",
          status: "active",
          lead: { agent_id: "ag_ernesto", name: "Ernesto", status: "working", uac_state: "safe", pane_id: "w1:p3" },
        },
        {
          id: "prj_hn",
          name: "herdr-notify",
          root_label: "herdr-notify",
          status: "active",
          lead: { agent_id: "ag_hugo", name: "Hugo", status: "idle", uac_state: "unchecked", pane_id: "w1:p20" },
        },
      ],
      alfred: { agent_id: "ag_alfred", name: "Alfred", status: "idle", uac_state: "safe" },
      unread: { conversations: 1, messages: 1, approvals: 0 },
      chat: { revision, unread: 1, pending_approvals: 0, active_delegations: 3 },
    });
  }
  if (path === "/v1/chat/conversations") {
    const archived = url.searchParams.get("archived") === "true";
    const q = (url.searchParams.get("q") || "").toLowerCase();
    const rows = archived
      ? []
      : conversations.filter((row) => !q || row.title.toLowerCase().includes(q));
    return json(response, 200, { ok: true, conversations: rows });
  }
  if (path === "/v1/chat/sections") return json(response, 200, { ok: true, sections: [] });
  if (path.endsWith("/read")) {
    return json(response, 200, { ok: true, conversation_id: demo, last_read_seq: 13 });
  }
  if (path.endsWith("/messages") && request.method === "POST") {
    return json(response, 202, {
      ok: true,
      message: {
        id: "msg_new",
        conversation_id: demo,
        seq: 99,
        author_kind: "user",
        body_md: "Mensaje de prueba",
        state: "complete",
        created_at: now,
      },
      turn: { id: "run_new", state: "queued", agent_id: "ag_alfred" },
    });
  }
  const messageMatch = path.match(/^\/v1\/chat\/conversations\/([^/]+)\/messages$/);
  if (messageMatch) {
    const rows = messages.get(messageMatch[1]) || [];
    return json(response, 200, { ok: true, messages: rows, next_before_seq: null });
  }
  const timelineMatch = path.match(/^\/v1\/chat\/conversations\/([^/]+)\/timeline$/);
  if (timelineMatch) {
    return json(response, 200, { ok: true, entries: timeline(timelineMatch[1]), next_before_seq: null });
  }
  const cancelConversation = path.match(/^\/v1\/chat\/conversations\/([^/]+)\/cancel$/);
  if (cancelConversation) {
    const ids = [];
    for (const { card, conversation } of cards.values()) {
      if (conversation === cancelConversation[1] && ["queued", "running", "waiting"].includes(card.status)) {
        ids.push(card.delegation_id);
        refreshCard(card.delegation_id, "cancelled", `${card.title}`, card.summary);
      }
    }
    return json(response, 200, { ok: true, conversation_id: cancelConversation[1], cancelled: ids });
  }
  const cancelDelegation = path.match(/^\/v1\/chat\/delegations\/([^/]+)\/cancel$/);
  if (cancelDelegation) {
    const id = cancelDelegation[1];
    const detail = details.get(id);
    if (!detail) return json(response, 404, { detail: "no_such_delegation" });
    if (!["queued", "running", "waiting"].includes(detail.delegation.state)) {
      return json(response, 409, { detail: "delegation_cancelled" });
    }
    const updated = refreshCard(id, "cancelled", `${detail.delegation.to_name} cancelado`, detail.delegation.task);
    return json(response, 200, { ok: true, ...updated });
  }
  const delegationMatch = path.match(/^\/v1\/chat\/delegations\/([^/]+)$/);
  if (delegationMatch) {
    const detail = details.get(delegationMatch[1]);
    return detail ? json(response, 200, { ok: true, ...detail }) : json(response, 404, { detail: "no_such_delegation" });
  }
  const conversationMatch = path.match(/^\/v1\/chat\/conversations\/([^/]+)$/);
  if (conversationMatch) {
    const conversation = conversations.find((row) => row.id === conversationMatch[1]);
    return conversation
      ? json(response, 200, {
          ok: true,
          conversation,
          participants: [{ agent_id: "ag_alfred", name: "Alfred", status: "idle", uac_state: "safe" }],
        })
      : json(response, 404, { detail: "no_such_conversation" });
  }
  json(response, 404, { detail: "not_found" });
});

server.listen(port, "127.0.0.1", () => console.log(`fake team host on http://127.0.0.1:${port}`));
