// The history chips that the host filters (Pendientes de ti, Trabajando) and
// their counts, free of the DOM so the tests can run them. `counts` is the
// reply of `GET …/counts`: null on an older host, undefined until it loads.

export const FILTERS = [
  { id: "all", label: "Todos" },
  { id: "attention", label: "Pendientes de ti" },
  { id: "working", label: "Trabajando" },
];

export function chipCount(count) {
  if (!count) return "";
  return count > 99 ? "99+" : String(count);
}

// An older host hides both new chips: a lone «Todos» would filter nothing.
export function chipsFor(counts) {
  if (counts === null) return [];
  return FILTERS.map((filter) => {
    const count = counts ? counts[filter.id] || 0 : 0;
    return { ...filter, count, text: chipCount(count) };
  });
}

// The accessible name keeps the exact number that the chip shows as «99+».
export function chipName(chip) {
  return chip.count ? `${chip.label}, ${chip.count}` : chip.label;
}

export const EMPTY_TEXT = {
  attention: "Nada pendiente de ti.",
  working: "Ningún agente está trabajando ahora.",
};

// The request for a filter the host answers; «Todos» reads the home list.
export function filterArgs(filter) {
  return filter === "all" ? null : { filter };
}
