import { app, $, invoke, listen, showView } from "./core.js";
import { errorMessage } from "./format.js";
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
import { inboxOnEvent, refreshInbox } from "./approval-inbox.js";
import {
  chatBoot,
  chatOnConnection,
  chatOnEvent,
  chatOnStateChange,
  installChatHandlers,
} from "./chat.js";

async function boot() {
  installChatHandlers();
  installPairHandlers();
  installAgentHandlers();
  installDetailHandlers();
  installNewConversationHandlers();
  installApprovalHandlers();
  mountApprovalSettings();
  $("btn-stale-pair").addEventListener("click", forgetPairing);
  $("btn-empty-pair").addEventListener("click", forgetPairing);
  $("btn-new").addEventListener("click", () => openNewConversation());
  $("btn-empty-new").addEventListener("click", () => openNewConversation());
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
    if (app.connection.kind === "connected") refreshInbox();
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
    if (state.state) app.state = state.state;
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
