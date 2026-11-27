<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let {
    globalOffsetMs,
    trackOffsetMs,
    hasTrack,
    onGlobal,
  }: { globalOffsetMs: number; trackOffsetMs: number; hasTrack: boolean; onGlobal: (ms: number) => void } = $props();

  const fmt = (ms: number) => `${ms > 0 ? "+" : ms < 0 ? "−" : "±"}${Math.abs(ms)} ms`;
  const nudge = (d: number) => invoke("nudge_track_offset", { deltaMs: d });
</script>

<section class="card">
  <header>
    <h2>Timing</h2>
    <p class="hint">Positive values show lines earlier.</p>
  </header>

  <div class="row">
    <label for="global">All songs</label>
    <output class="num">{fmt(globalOffsetMs)}</output>
  </div>
  <input
    id="global"
    type="range"
    min="-2000"
    max="2000"
    step="50"
    value={globalOffsetMs}
    oninput={(e) => onGlobal(Number((e.currentTarget as HTMLInputElement).value))}
    ondblclick={() => onGlobal(0)}
  />

  <div class="row track">
    <span class="label">This song</span>
    <output class="num">{fmt(trackOffsetMs)}</output>
  </div>
  <div class="nudges">
    <button class="btn" disabled={!hasTrack} onclick={() => nudge(-100)} aria-label="−100 ms">−100</button>
    <button class="btn" disabled={!hasTrack} onclick={() => nudge(100)} aria-label="+100 ms">+100</button>
    <button class="btn ghost" disabled={!hasTrack || trackOffsetMs === 0} onclick={() => invoke("reset_track_offset")}>Reset</button>
  </div>
</section>

<style>
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: var(--sp-2);
  }
  .row.track {
    margin-top: var(--sp-4);
  }
  label,
  .label {
    color: var(--ink-2);
    font-size: 13px;
  }
  .nudges {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: var(--sp-2);
  }
</style>
