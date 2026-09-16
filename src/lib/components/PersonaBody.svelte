<script lang="ts">
  /**
   * PersonaBody — an agent's identity + persona, rendered for the detail pane.
   * Extracted from the former PersonaDiscover slide-over so the unified Agents
   * workspace and any future surface share ONE persona renderer.
   *
   * Shows the header (emoji · name · category pill · vibe · description) and the
   * markdown persona body via our deterministic, escaping renderer. `agent` may
   * be a list-view stub (no body) shown instantly while the full record loads;
   * `loading` drives the body's skeleton until corpus.get() resolves.
   */
  import type { Snippet } from "svelte";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Pill from "./Pill.svelte";
  import LoadingState from "./LoadingState.svelte";
  import AgentOperationalCard from "./AgentOperationalCard.svelte";
  import { corpus } from "$lib/stores/corpus.svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import { resolveCategoryIcon } from "$lib/util/categoryIcon";
  import { renderMarkdown } from "$lib/util/markdown";
  import type { Agent } from "$lib/types";

  let {
    agent,
    loading = false,
    deploy,
    headerAction,
    onCategory,
  }: {
    agent: Agent | null;
    loading?: boolean;
    /** Optional deployment band rendered directly under the name/division,
        above the vibe + persona body (the deployment pills). */
    deploy?: Snippet;
    /** Optional action at the right of the title row (e.g. the Install… button). */
    headerAction?: Snippet;
    /** When provided, the category ("division") pill becomes a button that
        deep-links to that division. */
    onCategory?: (slug: string) => void;
  } = $props();

  // Rendered, deterministic HTML for the persona markdown body.
  let bodyHtml = $derived(renderMarkdown(agent?.body ?? ""));

  // Two-layer presentation (Review Packet §49/§53): agents with a
  // frontmatter `operational` block get a compact OPERATE card shown by
  // default, with the full prompt/identity/rules content collapsed behind
  // a "Details" disclosure. Agents WITHOUT that block (every agent that
  // predates this feature, in every other division) render exactly as
  // before — no card, no disclosure, `detailsOpen` is irrelevant to them
  // since the block below is only reached when `agent.operational` exists.
  let detailsOpen = $state(false);
  $effect(() => {
    // Reset the disclosure state when the selected agent changes, so
    // switching agents doesn't carry over "Details was open" from
    // whichever agent was open before.
    void agent?.slug;
    detailsOpen = false;
  });
</script>

{#if agent}
  {@const DivIcon = resolveCategoryIcon(corpus.iconOf(agent.category))}
  <header class="pb-head">
    <span class="pb-emoji" aria-hidden="true">{agent.emoji ?? "🧩"}</span>
    <div class="pb-titles">
      <h2 class="pb-name">{agent.name}</h2>
      <span class="pb-cat">
        {#if onCategory}
          <button class="pb-cat-btn" onclick={() => onCategory(agent.category)} title={i18n.t("coverage.seeDivision", { division: corpus.labelOf(agent.category) })}>
            <Pill tone="brand">
              <span class="pb-cat-ic" style="color:{corpus.colorOf(agent.category)}"><DivIcon size={12} /></span>{corpus.labelOf(agent.category)}
            </Pill>
          </button>
        {:else}
          <Pill tone="brand">
            <span class="pb-cat-ic" style="color:{corpus.colorOf(agent.category)}"><DivIcon size={12} /></span>{corpus.labelOf(agent.category)}
          </Pill>
        {/if}
      </span>
    </div>
    {#if headerAction}<div class="pb-action">{@render headerAction()}</div>{/if}
  </header>

  {#if deploy}
    <div class="pb-deploy">{@render deploy()}</div>
  {/if}

  <div class="pb-body">
    {#if agent.operational}
      <AgentOperationalCard operational={agent.operational} />

      <button
        class="pb-details-toggle"
        onclick={() => (detailsOpen = !detailsOpen)}
        aria-expanded={detailsOpen}
        aria-controls="pb-details-panel"
      >
        <ChevronDown size={14} class={detailsOpen ? "pb-chev open" : "pb-chev"} />
        {i18n.t("persona.detailsLabel")}
        <span class="pb-details-hint">{i18n.t("persona.detailsHint")}</span>
      </button>
    {/if}

    {#if !agent.operational || detailsOpen}
      <div id="pb-details-panel">
        {#if agent.vibe}
          <p class="pb-vibe">{agent.vibe}</p>
        {/if}
        {#if agent.description}
          <p class="pb-desc">{agent.description}</p>
        {/if}

        <div class="pb-persona">
          {#if loading}
            <LoadingState rows={5} label={i18n.t("persona.loading")} />
          {:else if bodyHtml}
            <!-- Markdown rendered by our deterministic, escaping renderer
                 (util/markdown.ts) — the sole source of this HTML, so {@html}
                 is safe here. -->
            <div class="markdown">{@html bodyHtml}</div>
          {:else}
            <p class="text-muted">{i18n.t("persona.emptyBody")}</p>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .pb-action { margin-left: auto; flex: none; }
  .pb-head {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }
  .pb-emoji { font-size: 28px; line-height: 1.1; flex: none; }
  .pb-titles { display: flex; flex-direction: column; gap: 6px; min-width: 0; flex: 1; }
  .pb-name {
    margin: 0;
    font-size: var(--text-h2, 1.2rem);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
  }
  .pb-cat { display: inline-flex; }
  /* Division icon tinted with its brand color, as the pill's leading glyph. */
  .pb-cat-ic { display: inline-flex; align-items: center; margin-right: 4px; }
  .pb-cat-btn { background: transparent; border: 0; padding: 0; cursor: pointer; display: inline-flex; border-radius: var(--radius-full); }
  .pb-cat-btn:hover { filter: brightness(1.12); }
  .pb-cat-btn:focus-visible { outline: 2px solid var(--color-focus, var(--color-brand)); outline-offset: 2px; }

  /* Deployment band — sits between the header and the persona text, so "where
     it's installed" reads right under the name/division. */
  .pb-deploy {
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }

  .pb-body {
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .pb-vibe {
    margin: 0;
    font-size: var(--text-body);
    font-style: italic;
    color: var(--color-text-secondary);
  }
  .pb-desc {
    margin: 0;
    font-size: var(--text-body-sm);
    color: var(--color-text-primary);
    line-height: var(--lh-normal, 1.5);
  }
  .pb-persona { margin-top: var(--space-2); }

  .pb-details-toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    align-self: flex-start;
    padding: var(--space-1) 0;
    background: transparent;
    border: 0;
    cursor: pointer;
    font-size: var(--text-body-sm);
    font-weight: var(--fw-medium);
    color: var(--color-text-secondary);
  }
  .pb-details-toggle:hover { color: var(--color-text-primary); }
  .pb-details-hint {
    font-weight: var(--fw-regular);
    font-size: var(--text-caption);
    color: var(--color-text-muted);
  }
  :global(.pb-chev) {
    flex: none;
    color: var(--color-text-muted);
    transition: transform var(--motion-duration-fast, 120ms) ease;
    transform: rotate(-90deg);
  }
  :global(.pb-chev.open) { transform: rotate(0deg); }

  /* Rendered-markdown typography. Scoped to .markdown so it only styles the
     persona body our renderer emits. */
  .markdown :global(h1),
  .markdown :global(h2),
  .markdown :global(h3),
  .markdown :global(h4),
  .markdown :global(h5),
  .markdown :global(h6) {
    margin: var(--space-4) 0 var(--space-2);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
    line-height: 1.3;
  }
  .markdown :global(h1) { font-size: var(--text-h3, 1.05rem); }
  .markdown :global(h2) { font-size: var(--text-body); }
  .markdown :global(h3),
  .markdown :global(h4),
  .markdown :global(h5),
  .markdown :global(h6) { font-size: var(--text-body-sm); }
  .markdown :global(p) {
    margin: 0 0 var(--space-3);
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-relaxed, 1.6);
  }
  .markdown :global(ul),
  .markdown :global(ol) {
    margin: 0 0 var(--space-3) var(--space-4);
    padding: 0;
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-relaxed, 1.6);
  }
  .markdown :global(li) { margin: 2px 0; list-style: disc; }
  .markdown :global(ol li) { list-style: decimal; }
  .markdown :global(hr) {
    border: 0;
    border-top: 1px solid var(--color-border);
    margin: var(--space-4) 0;
  }
  .markdown :global(code) {
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.85em;
    background: var(--color-surface-sunken);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
  }
  .markdown :global(pre) {
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    overflow-x: auto;
    margin: 0 0 var(--space-3);
  }
  .markdown :global(pre code) { background: transparent; padding: 0; }
  .markdown :global(a) { color: var(--color-brand); text-decoration: underline; }
  .markdown :global(strong) { color: var(--color-text-primary); font-weight: var(--fw-semibold); }
</style>
