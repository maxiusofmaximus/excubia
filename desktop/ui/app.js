import { app, $, invoke, listen } from "./core.js";
import { installNav, showView } from "./nav.js";
import { errorMessage, readableState } from "./format.js";
import {
  applyHostStatus,
  forgetPairing,
  hydratePairing,
  installPairHandlers,
  showPairError,
} from "./pair.js";
import {
  connectNow,
  installAgentHandlers,
  renderAgents,
  renderConnection,
  setState,
} from "./agents.js";
import { installDetailHandlers, openDetail, syncDetailFromState } from "./detail.js";
import { installNewConversationHandlers, openNewConversation } from "./newconversation.js";
import { installApprovalHandlers } from "./approvals.js";
import { mountApprovalSettings } from "./approval-settings.js";
import { mountMailSettings } from "./mail.js";
import { installMailDraft } from "./mail-draft.js";
import { inboxOnEvent, refreshInbox } from "./approval-inbox.js";
import { installTeam, refreshTeam } from "./hires.js";
import { paintChatSuggestions } from "./chatthread.js";
import { checkOnboarding, installOnboarding } from "./onboarding.js";
import { installPalette } from "./palette.js";
import { chatBoot, chatOnConnection, chatOnEvent, chatOnStateChange } from "./chat.js";
import { installChatHandlers } from "./chatinstall.js";
import { installAgenda } from "./agenda.js";
import { openDevices } from "./devices.js";
import { openContacts } from "./agenda-contacts.js";
import { openNotes } from "./agenda-notes.js";
import { openCalendar } from "./calendar.js";
import { openAlerts } from "./alerts.js";
import { installSettings, openSettings } from "./settings.js";

// The team shows in Equipo and in the suggestions of the empty chat.
function teamChanged() {
  renderAgents();
  paintChatSuggestions();
}

async function boot() {
  installChatHandlers();
  installPairHandlers();
  installAgentHandlers();
  installDetailHandlers();
  installNewConversationHandlers();
  installTeam(teamChanged);
  installPalette();
  installOnboarding();
  installApprovalHandlers();
  mountApprovalSettings();
  mountMailSettings();
  installMailDraft();
  installAgenda();
  installSettings();
  installNav({
    chat: () => showView("chat"),
    agents: () => showView("agents"),
    calendar: openCalendar,
    notes: openNotes,
    contacts: openContacts,
    alerts: openAlerts,
    devices: openDevices,
    settings: openSettings,
  });
  $("btn-stale-pair").addEventListener("click", forgetPairing);
  $("btn-empty-pair").addEventListener("click", forgetPairing);
  $("btn-new").addEventListener("click", () => openNewConversation());
  $("btn-new-here").addEventListener("click", () =>
    openNewConversation(app.current ? app.current.cwd || "" : ""),
  );
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !$("view-detail").classList.contains("hidden")) {
      app.detailToken += 1;
      const target = app.detailReturn === "chat" ? "chat" : "agents";
      app.detailReturn = null;
      showView(target);
    }
  });

  if (!invoke || !listen) {
    showView("pair");
    showPairError("No se pudo cargar el puente de Tauri. Abre la aplicación de escritorio.");
    return;
  }

  await listen("hn-connection", (event) => {
    app.connection = event.payload || { kind: "idle" };
    renderConnection();
    renderAgents();
    chatOnConnection();
    if (app.connection.kind === "connected") {
      refreshInbox();
      refreshTeam().then(teamChanged);
      checkOnboarding();
    }
  });
  await listen("hn-state", (event) => {
    setState(event.payload);
    syncDetailFromState();
    chatOnStateChange();
  });
  await listen("hn-chat", (event) => {
    chatOnEvent(event.payload);
    inboxOnEvent(event.payload);
  });
  await listen("hn-notification-click", (event) => {
    app.detailReturn = "agents";
    showView("agents");
    if (event.payload) openDetail(event.payload);
  });
  await listen("hn-host", (event) => {
    applyHostStatus(event.payload);
    renderAgents();
    chatOnConnection();
  });

  try {
    const state = await invoke("bootstrap");
    app.boot = state;
    app.connection = state.connection || { kind: "idle" };
    app.state = readableState(state.state);
    hydratePairing(state);
    renderConnection();
    renderAgents();

    if (state.mode === "local") {
      // The backend owns the local host: it pairs itself and connects.
      showView("chat");
      chatBoot();
    } else if (state.paired) {
      showView("chat");
      connectNow();
      chatBoot();
    } else {
      showView("pair");
      $("pair-code").focus();
    }
  } catch (error) {
    showView("pair");
    showPairError(errorMessage(error));
  }
}

boot();
