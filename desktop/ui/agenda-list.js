// The list both agenda screens share: a search, pages of rows with «Cargar más», and
// the states a read can show. A screen gives its loader and its row; this owns the rest.
// Pages stay in memory only; a stale page never replaces a newer search.

import { errorState } from "./empty-state.js";

// `emptyNode(query)` returns the screen's empty state; a failed read gets its error state.
export function createAgendaList({ list, status, more, fetchPage, renderRow, emptyNode, loadingText, failureText }) {
  let query = "";
  let cursor = null;
  let items = [];
  let token = 0;

  function setStatus(content) {
    status.replaceChildren();
    status.hidden = !content;
    if (content) status.append(content);
  }

  function render() {
    list.replaceChildren(...items.map(renderRow));
    more.hidden = !cursor;
    if (items.length) setStatus("");
    else setStatus(emptyNode(query));
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
      if (current === token) setStatus(errorState(failureText(failure), () => reload(query)));
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
      if (current === token) setStatus(errorState(failureText(failure), () => reload(query)));
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
