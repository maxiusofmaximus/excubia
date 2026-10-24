import { $ } from "./core.js";

const dayNames = ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"];
const monthNames = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];

export function formatTime(iso) {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.valueOf())) return "";
  return date.toLocaleTimeString("es-ES", { hour: "2-digit", minute: "2-digit" });
}

export function formatWhen(iso) {
  const date = iso ? new Date(iso) : null;
  if (!date || Number.isNaN(date.valueOf())) return "";
  const now = new Date();
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).valueOf();
  const value = date.valueOf();
  const dayMs = 86_400_000;
  if (value >= midnight) return formatTime(iso);
  if (value >= midnight - dayMs) return "ayer";
  if (value >= midnight - 6 * dayMs) return dayNames[date.getDay()];
  if (date.getFullYear() === now.getFullYear()) {
    return `${date.getDate()} ${monthNames[date.getMonth()]}`;
  }
  return `${date.getDate()} ${monthNames[date.getMonth()]} ${date.getFullYear()}`;
}

function row(conversation, selectedId) {
  const element = document.createElement("div");
  element.className = "chat-row";
  element.dataset.id = conversation.id;
  if (conversation.id === selectedId) element.classList.add("is-active");
  if (conversation.unread > 0) element.classList.add("is-unread");

  const main = document.createElement("button");
  main.type = "button";
  main.className = "chat-row-main";
  main.dataset.select = conversation.id;
  main.setAttribute(
    "aria-label",
    `${conversation.title}${conversation.unread ? `, ${conversation.unread} sin leer` : ""}`,
  );

  const title = document.createElement("span");
  title.className = "chat-row-title";
  title.textContent = `${conversation.pinned ? "★ " : ""}${conversation.title}`;
  title.title = conversation.title;

  const meta = document.createElement("span");
  meta.className = "chat-row-meta";
  const when = formatWhen(conversation.last_message_at);
  const parts = [];
  if (conversation.archived) parts.push("archivado");
  if (when) parts.push(when);
  if (conversation.unread) parts.push(`${conversation.unread} sin leer`);
  meta.textContent = parts.length ? `Alfred · ${parts.join(" · ")}` : "Sin mensajes";
  main.append(title, meta);

  if (conversation.unread) {
    const dot = document.createElement("span");
    dot.className = "unread-dot";
    dot.setAttribute("aria-hidden", "true");
    element.append(dot);
  }

  const more = document.createElement("button");
  more.type = "button";
  more.className = "row-more";
  more.dataset.menu = conversation.id;
  more.setAttribute("aria-label", `Acciones de ${conversation.title}`);
  more.setAttribute("aria-haspopup", "menu");
  more.textContent = "⋯";

  element.append(main, more);
  return element;
}

function caption(title) {
  const element = document.createElement("section");
  element.className = "group group-caption";
  const head = document.createElement("div");
  head.className = "group-head";
  head.textContent = title;
  element.append(head);
  return element;
}

function group(title, conversations, selectedId, options = {}) {
  const element = document.createElement("section");
  element.className = "group";
  const head = document.createElement("div");
  head.className = "group-head";
  const label = document.createElement("span");
  label.textContent = title;
  head.append(label);
  if (options.action) {
    const action = document.createElement("button");
    action.type = "button";
    action.className = "group-action";
    action.dataset.action = options.action;
    action.textContent = options.actionLabel || "Ver";
    head.append(action);
  }
  element.append(head);
  for (const conversation of conversations) {
    element.append(row(conversation, selectedId));
  }
  return element;
}

export function renderHistory(chat) {
  const groups = $("chat-groups");
  groups.replaceChildren();
  const home = chat.home;
  const selectedId = chat.conversation ? chat.conversation.id : null;

  if (!home) {
    for (let index = 0; index < 4; index += 1) {
      const skeleton = document.createElement("div");
      skeleton.className = "skeleton-chat";
      groups.append(skeleton);
    }
    $("history-count").textContent = "0";
    $("btn-archived").textContent = "Archivados (0)";
    return;
  }

  const conversations = home.conversations || [];
  const archived = home.archived || [];
  $("history-count").textContent = String(conversations.length);

  if (chat.searchResults) {
    if (!chat.searchResults.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = `Sin resultados para «${chat.query}».`;
      groups.append(empty);
    } else {
      groups.append(
        group(`Resultados · ${chat.searchResults.length}`, chat.searchResults, selectedId),
      );
    }
    return;
  }

  if (chat.showArchived) {
    if (!archived.length) {
      const empty = document.createElement("p");
      empty.className = "history-empty";
      empty.textContent = "No hay conversaciones archivadas.";
      groups.append(empty);
    } else {
      groups.append(group("Archivados", archived, selectedId));
    }
    return;
  }

  if (!conversations.length) {
    const empty = document.createElement("p");
    empty.className = "history-empty";
    empty.textContent = "Aún no hay conversaciones. Empieza con «Nuevo chat».";
    groups.append(empty);
    return;
  }

  const pinned = conversations.filter((conversation) => conversation.pinned);
  const sections = home.sections || [];
  const sectionIds = new Set(sections.map((section) => section.id));
  const inSections = conversations.filter(
    (conversation) => !conversation.pinned && conversation.section_id && sectionIds.has(conversation.section_id),
  );
  const recent = conversations.filter(
    (conversation) => !conversation.pinned && !inSections.includes(conversation),
  );

  if (pinned.length) groups.append(group("Fijados", pinned, selectedId));
  if (recent.length) groups.append(group("Recientes", recent, selectedId));

  if (sections.length) {
    groups.append(caption("Secciones"));
    for (const section of sections) {
      const rows = inSections.filter((conversation) => conversation.section_id === section.id);
      groups.append(group(section.name, rows, selectedId));
    }
  }
  if (archived.length) {
    groups.append(
      group(`Archivados · ${archived.length}`, [], selectedId, {
        action: "toggle-archived",
        actionLabel: "Ver",
      }),
    );
  }
}

const SEAT_STATUS = {
  draft: "disponible",
  idle: "disponible",
  ready: "disponible",
  working: "trabajando",
  running: "trabajando",
  blocked: "bloqueado",
  done: "terminó",
};

function splitSeat(title) {
  const match = String(title || "").match(/^(.*?)\s+[—-]\s+(.*)$/);
  return match ? [match[1], match[2]] : null;
}

function seatLabel(agent) {
  const title = (agent.title || "").trim();
  const split = splitSeat(title);
  if (split) return split[0];
  if (title && !/[\\/]/.test(title) && title.length <= 40) return title;
  return agent.agent || "Especialista";
}

function seatRole(agent) {
  const split = splitSeat((agent.title || "").trim());
  if (split) return split[1];
  return agent.agent || "especialista";
}

function matchesProject(agent, project) {
  const projectName = (project.name || "").trim().toLowerCase();
  const root = (project.root_label || "").trim().toLowerCase();
  const stateProject = (agent.project || "").trim().toLowerCase();
  if (stateProject && (stateProject === projectName || stateProject === root)) return true;
  if (!root) return false;
  const cwd = (agent.cwd || "").replace(/\\/g, "/").toLowerCase();
  return cwd.endsWith(`/${root}`);
}

function agentButton(agent, role, extraClass = "") {
  const row = document.createElement("button");
  row.type = "button";
  row.className = `side-row ${extraClass}`;
  row.dataset.pane = agent.pane_id;
  row.setAttribute(
    "aria-label",
    `${seatLabel(agent)}, ${role}, ${SEAT_STATUS[agent.status] || agent.status}`,
  );
  const dot = document.createElement("span");
  dot.className = `status-dot status-${agent.status}`;
  dot.setAttribute("aria-hidden", "true");
  const name = document.createElement("span");
  name.className = "side-name";
  name.textContent = seatLabel(agent);
  const roleLabel = document.createElement("span");
  roleLabel.className = "side-role";
  roleLabel.textContent = role;
  row.append(dot, name, roleLabel);
  return row;
}

export function renderSide(chat, agents) {
  const bootstrap = chat.home ? chat.home.bootstrap : null;
  const alfred = bootstrap ? bootstrap.alfred : null;
  $("side-alfred-name").textContent = alfred ? alfred.name : "Alfred";
  $("side-alfred-status").textContent = alfred
    ? `CTO · ${SEAT_STATUS[alfred.status] || alfred.status || "disponible"}`
    : "Sin conexión";
  $("side-company").textContent = bootstrap
    ? `${bootstrap.company.name}${bootstrap.chat ? ` · ${bootstrap.chat.unread} sin leer` : ""}`
    : "Sin datos del host";

  const live = agents || [];
  const projects = $("side-projects");
  projects.replaceChildren();
  const projectRows = bootstrap ? bootstrap.projects || [] : [];

  if (!projectRows.length) {
    const empty = document.createElement("p");
    empty.className = "side-empty";
    empty.textContent = "Sin proyectos todavía.";
    projects.append(empty);
  } else {
    for (const project of projectRows) {
      const card = document.createElement("div");
      card.className = "project-card";
      const head = document.createElement("div");
      head.className = "project-head";
      const name = document.createElement("span");
      name.className = "project-name";
      name.textContent = project.name;
      const state = document.createElement("span");
      state.className = "project-state";
      state.textContent = project.status === "active" ? "activo" : project.status || "proyecto";
      head.append(name, state);
      card.append(head, Object.assign(document.createElement("p"), {
        className: "project-root",
        textContent: project.root_label || "",
      }));

      const team = document.createElement("div");
      team.className = "project-team";
      const lead = project.lead;
      if (lead) {
        const leadAgent = lead.pane_id ? live.find((agent) => agent.pane_id === lead.pane_id) : null;
        if (leadAgent) {
          team.append(
            agentButton(
              { ...leadAgent, title: `${lead.name} — lead`, agent: "lead" },
              "lead",
              "project-lead",
            ),
          );
        } else {
          const row = document.createElement("div");
          row.className = "side-row project-lead is-offline";
          const dot = document.createElement("span");
          dot.className = `status-dot status-${lead.status || "unknown"}`;
          dot.setAttribute("aria-hidden", "true");
          const label = document.createElement("span");
          label.className = "side-name";
          label.textContent = lead.name || "Lead";
          const role = document.createElement("span");
          role.className = "role-badge";
          role.textContent = "lead sin panel";
          row.append(dot, label, role);
          team.append(row);
        }
      }
      const specialists = lead
        ? live.filter(
            (agent) =>
              agent.pane_id !== lead.pane_id && matchesProject(agent, project),
          )
        : live.filter((agent) => matchesProject(agent, project));
      for (const agent of specialists) {
        team.append(agentButton(agent, seatRole(agent)));
      }
      if (!lead && !specialists.length) {
        const empty = document.createElement("p");
        empty.className = "side-empty-project";
        empty.textContent = "Sin equipo con panel.";
        team.append(empty);
      }
      card.append(team);
      projects.append(card);
    }
  }

  const team = $("side-team");
  const section = $("side-team-section");
  team.replaceChildren();
  section.classList.toggle("hidden", projectRows.length > 0);
  if (projectRows.length) return;
  if (!live.length) {
    const empty = document.createElement("p");
    empty.className = "side-empty";
    empty.textContent = alfred
      ? `${alfred.name} está solo: aún no hay paneles de agentes.`
      : "El equipo aparecerá aquí.";
    team.append(empty);
    return;
  }
  for (const agent of live) {
    team.append(agentButton(agent, agent.project || "panel"));
  }
}
