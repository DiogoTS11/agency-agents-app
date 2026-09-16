<script lang="ts">
  /**
   * AgentOperationalCard — the "OPERATE" layer for an agent's detail pane.
   *
   * Renders the compact, scannable summary from an agent's optional
   * `operational` frontmatter block (Review Packet §49 schema, §53
   * implementation). Shown ONLY when `agent.operational` is present —
   * PersonaBody decides that, this component just renders what it's given.
   *
   * Deliberately reuses existing primitives (Pill, design tokens) instead
   * of inventing new visual language, per the "don't redesign the app"
   * constraint.
   */
  import Pill from "./Pill.svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import type { Operational } from "$lib/types";

  let { operational }: { operational: Operational } = $props();

  const hasSkills = $derived(
    (operational.skills?.primary?.length ?? 0) > 0 ||
      (operational.skills?.complementary?.length ?? 0) > 0 ||
      (operational.skills?.specialist?.length ?? 0) > 0,
  );
</script>

<section class="opcard" aria-label={i18n.t("persona.operateLabel")}>
  <div class="opcard-top">
    {#if operational.phase}
      <Pill tone="brand">{operational.phase}</Pill>
    {/if}
    {#if operational.deploymentStatus}
      <Pill tone="info">{operational.deploymentStatus}</Pill>
    {/if}
  </div>

  {#if operational.role}
    <p class="opcard-role">{operational.role}</p>
  {/if}

  {#if operational.whatItDoes}
    <p class="opcard-what">{operational.whatItDoes}</p>
  {/if}

  <div class="opcard-grid">
    {#if operational.whenToUse}
      <div class="opcard-field">
        <span class="opcard-label ok">{i18n.t("persona.whenToUse")}</span>
        <p class="opcard-text">{operational.whenToUse}</p>
      </div>
    {/if}
    {#if operational.whenNotToUse}
      <div class="opcard-field">
        <span class="opcard-label warn">{i18n.t("persona.whenNotToUse")}</span>
        <p class="opcard-text">{operational.whenNotToUse}</p>
      </div>
    {/if}
  </div>

  {#if hasSkills}
    <div class="opcard-field">
      <span class="opcard-label">{i18n.t("persona.skills")}</span>
      <div class="opcard-tags">
        {#each operational.skills?.primary ?? [] as s (s)}
          <Pill tone="brand">{s}</Pill>
        {/each}
        {#each operational.skills?.complementary ?? [] as s (s)}
          <Pill tone="info">{s}</Pill>
        {/each}
        {#each operational.skills?.specialist ?? [] as s (s)}
          <Pill tone="neutral">{s}</Pill>
        {/each}
      </div>
    </div>
  {/if}

  {#if operational.reviewer}
    <div class="opcard-field">
      <span class="opcard-label">{i18n.t("persona.reviewer")}</span>
      <p class="opcard-text">{operational.reviewer}</p>
    </div>
  {/if}

  {#if (operational.workflows?.length ?? 0) > 0}
    <div class="opcard-field">
      <span class="opcard-label">{i18n.t("persona.workflows")}</span>
      <div class="opcard-tags">
        {#each operational.workflows ?? [] as w (w)}
          <Pill tone="neutral">{w}</Pill>
        {/each}
      </div>
    </div>
  {/if}

  <div class="opcard-grid">
    {#if (operational.inputs?.length ?? 0) > 0}
      <div class="opcard-field">
        <span class="opcard-label">{i18n.t("persona.inputs")}</span>
        <ul class="opcard-list">
          {#each operational.inputs ?? [] as v (v)}<li>{v}</li>{/each}
        </ul>
      </div>
    {/if}
    {#if (operational.outputs?.length ?? 0) > 0}
      <div class="opcard-field">
        <span class="opcard-label">{i18n.t("persona.outputs")}</span>
        <ul class="opcard-list">
          {#each operational.outputs ?? [] as v (v)}<li>{v}</li>{/each}
        </ul>
      </div>
    {/if}
  </div>
</section>

<style>
  .opcard {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
  }
  .opcard-top { display: flex; gap: var(--space-2); flex-wrap: wrap; }
  .opcard-role {
    margin: 0;
    font-size: var(--text-body);
    font-weight: var(--fw-semibold);
    color: var(--color-text-primary);
  }
  .opcard-what {
    margin: 0;
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-normal, 1.5);
  }
  .opcard-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }
  @media (max-width: 480px) {
    .opcard-grid { grid-template-columns: 1fr; }
  }
  .opcard-field { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .opcard-label {
    font-size: var(--text-caption);
    font-weight: var(--fw-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-text-muted);
  }
  .opcard-label.ok { color: var(--color-success-on-subtle, var(--color-text-muted)); }
  .opcard-label.warn { color: var(--color-warning-on-subtle, var(--color-text-muted)); }
  .opcard-text {
    margin: 0;
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-normal, 1.5);
  }
  .opcard-tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .opcard-list {
    margin: 0;
    padding-left: 16px;
    font-size: var(--text-body-sm);
    color: var(--color-text-secondary);
    line-height: var(--lh-normal, 1.5);
  }
  .opcard-list li { margin: 2px 0; }
</style>
