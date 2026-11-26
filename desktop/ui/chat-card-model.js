// What the chat paints for a failed card and for the model under an answer. Pure, so the tests
// can check the words and labels without a DOM.

// The card's title: the words of its reason when the host gave a known one, else the generic title.
export function failedTitle(message, fallback) {
  return message.failure_text || fallback;
}

// The model that wrote an answer, shown small in its meta line. The provider prefix is dropped;
// an answer without a model has no label.
export function answerModel(message) {
  if (!message.model) return "";
  return message.model.replace(/^opencode\//, "");
}
