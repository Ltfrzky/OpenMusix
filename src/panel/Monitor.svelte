<script lang="ts">
  import type { Mode, Snapshot } from "../lib/types";
  import Overlay from "../overlay/Overlay.svelte";

  let { snapshot, mode, artBase }: { snapshot: Snapshot | null; mode: Mode; artBase: string } = $props();

  // The monitor emulates a 1920×1080 OBS canvas; the widget sits bottom-left at its 600×200 source size.
  let width = $state(960);
  const scale = $derived(width / 1920);
</script>

<div class="monitor" bind:clientWidth={width}>
  <div class="canvas" style="transform: scale({scale})">
    <div class="safe"></div>
    {#if mode === "focus"}
      <div class="source focus"><Overlay {snapshot} pinnedMode="focus" preview {artBase} /></div>
    {:else}
      <div class="source widget"><Overlay {snapshot} pinnedMode="widget" preview {artBase} /></div>
    {/if}
  </div>
  <span class="badge">Preview · 1080p</span>
</div>

<style>
  .monitor {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: var(--r-lg);
    overflow: hidden;
    background:
      radial-gradient(120% 90% at 30% 20%, #23252c 0%, #121317 60%),
      var(--booth-inset);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .canvas {
    position: absolute;
    top: 0;
    left: 0;
    width: 1920px;
    height: 1080px;
    transform-origin: 0 0;
  }
  .safe {
    position: absolute;
    inset: 54px 96px;
    border: 2px dashed rgba(255, 255, 255, 0.05);
    border-radius: 12px;
  }
  .source {
    position: absolute;
  }
  .source.focus {
    inset: 0;
  }
  .source.widget {
    left: 48px;
    bottom: 48px;
    width: 600px;
    height: 200px;
  }
  .badge {
    position: absolute;
    top: var(--sp-3);
    left: var(--sp-3);
    height: 22px;
    padding: 0 8px;
    display: inline-flex;
    align-items: center;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.45);
    color: var(--ink-3);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
</style>
