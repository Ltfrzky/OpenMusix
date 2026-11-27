<script lang="ts">
  import { onMount } from "svelte";
  import type { Mode, ServerStatus, Settings, Snapshot, Style } from "../lib/types";
  import { api, onServerStatus, onSnapshot } from "./api";
  import NowPlayingCard from "./NowPlayingCard.svelte";
  import Monitor from "./Monitor.svelte";
  import ModeToggle from "./ModeToggle.svelte";
  import OffsetControls from "./OffsetControls.svelte";
  import ObsUrl from "./ObsUrl.svelte";
  import StyleEditor from "./StyleEditor.svelte";
  import SettingsView from "./SettingsView.svelte";
  import PortBanner from "./PortBanner.svelte";
  import SpotifySetup from "./SpotifySetup.svelte";

  let snapshot = $state<Snapshot | null>(null);
  let settings = $state<Settings | null>(null);
  let server = $state<ServerStatus | null>(null);
  let tab = $state<"live" | "settings">("live");
  let focusPort = $state(0);

  const port = $derived(settings?.port ?? 7878);
  const mode: Mode = $derived(snapshot?.mode ?? settings?.mode ?? "focus");
  const artBase = $derived(`http://127.0.0.1:${port}`);

  onMount(() => {
    // Register listeners first, then read current state: an event fired in between is never lost.
    const snapOff = onSnapshot((s) => (snapshot = s));
    const serverOff = onServerStatus((s) => (server = s));
    snapOff.then(() => api.getSnapshot()).then((s) => (snapshot = s));
    serverOff.then(() => api.serverStatus()).then((s) => (server = s));
    api.getSettings().then((s) => (settings = s));
    return () => [snapOff, serverOff].forEach((p) => p.then((off) => off()));
  });

  // Writes send only the fields that changed: the backend also changes settings (hotkey,
  // tray, per-song nudges), and a full object from here would overwrite those changes.
  // Style and slider edits are debounced so dragging stays smooth.
  let pending: Partial<Settings> = {};
  let timer: ReturnType<typeof setTimeout> | undefined;
  async function save(patch: Partial<Settings>): Promise<string | null> {
    if (!settings) return null;
    settings = { ...settings, ...patch };
    try {
      settings = await api.updateSettings(patch);
      return null;
    } catch (e) {
      settings = await api.getSettings();
      return String(e);
    }
  }
  function saveSoon(patch: Partial<Settings>) {
    if (!settings) return;
    settings = { ...settings, ...patch };
    pending = { ...pending, ...patch };
    clearTimeout(timer);
    timer = setTimeout(() => {
      const p = pending;
      pending = {};
      save(p);
    }, 150);
  }

  function setMode(m: Mode) {
    if (settings) settings = { ...settings, mode: m };
    api.setMode(m);
  }
  function setStyle(mode: Mode, s: Style) {
    if (!settings) return;
    saveSoon({ styles: { ...settings.styles, [mode]: s } });
  }
  function fixPort() {
    tab = "settings";
    focusPort++;
  }
</script>

<div class="shell">
  <nav class="rail">
    <div class="brand">
      <span class="mark" aria-hidden="true"><span class="icon">graphic_eq</span></span>
      <span class="name">OpenMusix</span>
    </div>
    <button class="nav" class:active={tab === "live"} onclick={() => (tab = "live")}><span class="icon">sensors</span>Live</button>
    <button class="nav" class:active={tab === "settings"} onclick={() => (tab = "settings")}><span class="icon">settings</span>Settings</button>
    <div class="grow"></div>
    <p class="status">
      <span class="led" class:ok={server?.state === "running"}></span>
      {server?.state === "running" ? `Overlay live on :${server.port}` : "Overlay offline"}
    </p>
  </nav>

  <main>
    <PortBanner status={server} onFix={fixPort} onRetry={() => api.restartServer().then((s) => (server = s))} />

    {#if !settings}
      <p class="hint">Loading…</p>
    {:else if tab === "live"}
      <div class="live">
        <div class="stage">
          <div class="cue">
            <NowPlayingCard {snapshot} {artBase} />
            <ModeToggle {mode} hotkey={settings.hotkey} onChange={setMode} />
          </div>
          <Monitor {snapshot} {mode} {artBase} />
          <ObsUrl {port} />
        </div>
        <aside class="controls">
          <OffsetControls
            globalOffsetMs={settings.globalOffsetMs}
            trackOffsetMs={snapshot?.trackOffsetMs ?? 0}
            hasTrack={!!snapshot?.trackKey}
            onGlobal={(ms) => saveSoon({ globalOffsetMs: ms })}
          />
          <StyleEditor {mode} style={settings.styles[mode]} onChange={(s) => setStyle(mode, s)} />
        </aside>
      </div>
    {:else}
      <SettingsView {settings} onSave={save} {focusPort}>
        {#snippet spotify()}
          <SpotifySetup settings={settings!} onSave={save} />
        {/snippet}
      </SettingsView>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 200px 1fr;
    height: 100vh;
  }
  .rail {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--sp-4) var(--sp-3);
    box-shadow: inset -1px 0 0 var(--rule-soft);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 0 var(--sp-2) var(--sp-5);
  }
  .mark {
    width: 28px;
    height: 28px;
    border-radius: var(--r-sm);
    display: grid;
    place-items: center;
    background: var(--tally);
    color: var(--tally-ink);
  }
  .name {
    font-family: var(--font-display);
    font-weight: 600;
    font-size: 16px;
    letter-spacing: -0.01em;
  }
  .nav {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: 40px;
    padding: 0 var(--sp-3);
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-3);
    font: 500 13px var(--font-ui);
    text-align: left;
    cursor: pointer;
    transition-property: background-color, color;
    transition-duration: 150ms;
  }
  .nav:hover {
    color: var(--ink-1);
  }
  .nav.active {
    background: var(--booth-raised);
    color: var(--ink-1);
    box-shadow: inset 0 0 0 1px var(--rule-soft);
  }
  .nav.active .icon {
    color: var(--tally);
  }
  .grow {
    flex: 1;
  }
  .status {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 0 var(--sp-2);
    font-size: 12px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .led {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--onair);
  }
  .led.ok {
    background: var(--live);
  }
  main {
    overflow: auto;
    padding: var(--sp-5);
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  .live {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 320px;
    gap: var(--sp-5);
    align-items: start;
  }
  .stage {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    min-width: 0;
  }
  .cue {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-4);
    flex-wrap: wrap;
  }
  .controls {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
</style>
