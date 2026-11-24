// The history chips that the host filters (Pendientes de ti, Trabajando) and
// their counts, free of the DOM so the tests can run them. `counts` is the
// reply of `GET …/counts`: null on an older host, undefined until it loads.

import { t } from "./i18n.js";

export const FILTERS = [
  { id: "all", label: t("drawer.filter_all") },
  { id: "attention", label: t("chatfilter.attention") },
  { id: "working", label: t("chatfilter.working") },
];

// Only «Pendientes de ti» shows a number, and only while it is above zero.
const COUNTED = new Set(["attention"]);

export function chipCount(count) {
  if (!count) return "";
  return count > 99 ? "99+" : String(count);
}

// An older host hides both new chips: a lone «Todos» would filter nothing.
export function chipsFor(counts) {
  if (counts === null) return [];
  return FILTERS.map((filter) => {
    const count = counts && COUNTED.has(filter.id) ? counts[filter.id] || 0 : 0;
    return { ...filter, count, text: chipCount(count) };
  });
}

// The accessible name keeps the exact number that the chip shows as «99+».
export function chipName(chip) {
  return chip.count ? `${chip.label}, ${chip.count}` : chip.label;
}

export const EMPTY_TEXT = {
  attention: t("chatfilter.empty_attention"),
  working: t("chatfilter.empty_working"),
};

// The request for a filter the host answers; «Todos» reads the home list.
export function filterArgs(filter) {
  return filter === "all" ? null : { filter };
}
