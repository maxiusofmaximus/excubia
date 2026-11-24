import { app, $, invoke, toast } from "./core.js";
import { openDetail } from "./detail.js";
import { openDelegationDetail } from "./delegations.js";
import { chat, failure } from "./chatstate.js";
import { saveRecipeFromDelegation } from "./recipes.js";
import { t } from "./i18n.js";

// The delegation cards' actions: cancel one, or the kill switch of the whole
// conversation. `render` redraws the chat and `refresh` reloads the feed.
export function createDelegations({ render, refresh }) {
  async function cancel(id) {
    chat.cancelTarget = null;
    try {
      await invoke("chat_cancel_delegation", { delegationId: id });
      chat.delegationError = null;
      toast(t("chatdelegations.cancelled"));
      await refresh();
    } catch (error) {
      const failureInfo = failure(error);
      chat.delegationError = failureInfo;
      toast(failureInfo.message);
      render();
    }
  }

  async function cancelAll(event) {
    event.preventDefault();
    $("delegation-kill").close();
    if (!chat.conversation) return;
    try {
      const outcome = await invoke("chat_cancel_conversation", {
        conversationId: chat.conversation.id,
      });
      chat.delegationError = null;
      toast(
        outcome.cancelled.length
          ? t("chatdelegations.cancelled_count", { n: outcome.cancelled.length })
          : t("chatdelegations.none_running"),
      );
      await refresh();
    } catch (error) {
      const failureInfo = failure(error);
      chat.delegationError = failureInfo;
      toast(failureInfo.message);
      render();
    }
  }

  function handleAction(dataset) {
    const action = dataset.delegationAction;
    if (action === "agent" && dataset.pane) {
      app.detailReturn = "chat";
      openDetail(dataset.pane).then((found) => {
        if (!found) toast(t("chatdelegations.panel_gone"));
      });
    } else if (action === "detail") {
      openDelegationDetail(dataset.delegation);
    } else if (action === "save-recipe") {
      saveRecipeFromDelegation(dataset.delegation);
    } else if (action === "cancel") {
      chat.cancelTarget = dataset.delegation;
      render();
    } else if (action === "cancel-abort") {
      chat.cancelTarget = null;
      render();
    } else if (action === "cancel-confirm") {
      cancel(dataset.delegation);
    }
  }

  return { cancelAll, handleAction };
}
