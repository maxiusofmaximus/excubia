// The list both agenda screens share: a search, pages of rows with «Cargar más», and
// the states a read can show. A screen gives its loader and its row; this owns the rest.
// Pages stay in memory only; a stale page never replaces a newer search.

import { element } from "./chatdom.js";

export function createAgendaList({ list, status, more, fetchPage, renderRow, emptyText, loadingText, failureText }) {
  let query = "";
  let cursor = null;
  let items = [];
  let token = 0;

  function setStatus(text, retry = false) {
    status.replaceChildren();
    status.hidden = !text;
    if (!text) return;
    status.append(text);
    if (retry) {
      const again = element("button", "btn secondary btn-small", "Reintentar");
      again.type = "button";
      again.addEventListener("click", () => reload(query));
      status.append(" ", again);
    }
  }

  function render() {
    list.replaceChildren(...items.map(renderRow));
    more.hidden = !cursor;
    if (items.length) setStatus("");
    else setStatus(emptyText(query));
  }

  async function reload(search = query) {
    query = search;
    const current = ++token;
    items = [];
    cursor = null;
    list.replaceChildren();
    more.hidden = true;
    setStatus(loadingText);
    try {
      const page = await fetchPage({ q: query, cursor: null });
      if (current !== token) return;
      items = page.items;
      cursor = page.next;
      render();
    } catch (failure) {
      if (current === token) setStatus(failureText(failure), true);
    }
  }

  async function loadMore() {
    if (!cursor) return;
    const current = token;
    more.disabled = true;
    try {
      const page = await fetchPage({ q: query, cursor });
      if (current !== token) return;
      items = items.concat(page.items);
      cursor = page.next;
      render();
    } catch (failure) {
      if (current === token) setStatus(failureText(failure), true);
    } finally {
      more.disabled = false;
    }
  }

  // A row written here updates the list without a new read.
  function upsert(item) {
    const index = items.findIndex((existing) => existing.id === item.id);
    if (index === -1) items = [item, ...items];
    else items = items.map((existing) => (existing.id === item.id ? item : existing));
    render();
  }

  function remove(id) {
    items = items.filter((existing) => existing.id !== id);
    render();
  }

  return { reload, loadMore, upsert, remove, redraw: render };
}
