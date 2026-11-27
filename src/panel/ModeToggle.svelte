<script lang="ts">
  import type { Mode } from "../lib/types";

  let { mode, hotkey, onChange }: { mode: Mode; hotkey: string; onChange: (m: Mode) => void } = $props();
  const options: { id: Mode; label: string; icon: string }[] = [
    { id: "focus", label: "Focus", icon: "lyrics" },
    { id: "widget", label: "Widget", icon: "picture_in_picture_alt" },
  ];
</script>

<div class="toggle-wrap">
  <div class="toggle" role="radiogroup" aria-label="Overlay layout">
    {#each options as o (o.id)}
      <button role="radio" aria-checked={mode === o.id} class:active={mode === o.id} onclick={() => onChange(o.id)}>
        <span class="icon">{o.icon}</span>{o.label}
      </button>
    {/each}
  </div>
  <span class="hint"><kbd>{hotkey}</kbd> switches live</span>
</div>

<style>
  .toggle-wrap {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .toggle {
    display: inline-flex;
    padding: 3px;
    border-radius: 11px;
    background: var(--booth-inset);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    padding: 0 var(--sp-3);
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-3);
    font: 500 13px var(--font-ui);
    cursor: pointer;
    transition-property: background-color, color, scale;
    transition-duration: 150ms;
    transition-timing-function: var(--ease);
  }
  button:hover {
    color: var(--ink-1);
  }
  button:active {
    scale: 0.96;
  }
  button.active {
    background: var(--booth-high);
    color: var(--ink-1);
    box-shadow: inset 0 0 0 1px var(--rule-strong);
  }
  button.active .icon {
    color: var(--tally);
  }
  kbd {
    font: 500 11px var(--font-ui);
    padding: 2px 6px;
    border-radius: 5px;
    box-shadow: inset 0 0 0 1px var(--rule-strong);
    color: var(--ink-2);
  }
</style>
