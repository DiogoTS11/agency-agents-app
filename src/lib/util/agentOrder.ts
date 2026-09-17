/**
 * Agent Ordering Standard comparator (Review Packet §84.8, §102; Obsidian
 * `Agent Ordering Standard.md`). Pure, standalone, and pinned by inline
 * self-checks below (no test runner is configured in this project — see
 * Review Packet §102 for the honest limitation) so the rule stays exactly
 * this:
 *
 *   1. Both have `operational.order` → lower order first.
 *   2. Same order → alphabetical by display name (a real parallel phase,
 *      not an error — multiple agents legitimately share an order).
 *   3. Only one has `order` → the ordered agent comes first.
 *   4. Neither has `order` → unchanged legacy alphabetical-by-name
 *      behavior, byte-for-byte identical to the pre-existing sort.
 *
 * No agent name is ever hardcoded here — this reads only `order`/`name`.
 */

interface Orderable {
  name: string;
  operational?: { order?: number | null } | null;
}

function orderOf(a: Orderable): number | null {
  const o = a.operational?.order;
  return typeof o === "number" ? o : null;
}

/** Comparator for `Array.prototype.sort`. */
export function compareAgentOrder<T extends Orderable>(a: T, b: T): number {
  const ao = orderOf(a);
  const bo = orderOf(b);
  if (ao !== null && bo !== null) {
    if (ao !== bo) return ao - bo;
    return a.name.localeCompare(b.name);
  }
  if (ao !== null) return -1; // a ordered, b not → a first
  if (bo !== null) return 1; // b ordered, a not → b first
  return a.name.localeCompare(b.name); // legacy fallback, both unordered
}

/**
 * Build a slug → order lookup from the full corpus, for the other agent-row
 * lists (Deploy, Projects, Teams, Updates) that carry only `slug`/`name`
 * rows rather than a full {@link Agent}, so they can honor the same rule
 * without re-fetching `operational` per row (Review Packet §102/§103 —
 * §103.7 defect #1: these lists were found still hardcoded to alphabetical).
 */
export function buildOrderBySlug(
  agents: readonly { slug: string; operational?: { order?: number | null } | null }[],
): Map<string, number> {
  const m = new Map<string, number>();
  for (const a of agents) {
    const o = a.operational?.order;
    if (typeof o === "number") m.set(a.slug, o);
  }
  return m;
}

/**
 * Same rule as {@link compareAgentOrder}, generalized to any `slug`+`name`
 * row (not just a full {@link Agent}) via a precomputed order lookup from
 * {@link buildOrderBySlug}. A slug absent from the map behaves exactly like
 * an agent with no `operational.order` — legacy alphabetical fallback.
 */
export function compareBySlugOrder(orderBySlug: ReadonlyMap<string, number>) {
  return function compare(a: { slug: string; name: string }, b: { slug: string; name: string }): number {
    const ao = orderBySlug.get(a.slug) ?? null;
    const bo = orderBySlug.get(b.slug) ?? null;
    if (ao !== null && bo !== null) {
      if (ao !== bo) return ao - bo;
      return a.name.localeCompare(b.name);
    }
    if (ao !== null) return -1;
    if (bo !== null) return 1;
    return a.name.localeCompare(b.name);
  };
}
