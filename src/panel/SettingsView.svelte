<script lang="ts">
  import type { Settings } from "../lib/types";
  import type { Snippet } from "svelte";
  import { api } from "./api";
  import RomanizationSection from "./RomanizationSection.svelte";

  let {
    settings,
    onSave,
    focusPort = 0,
    spotify,
  }: { settings: Settings; onSave: (patch: Partial<Settings>) => Promise<string | null>; focusPort?: number; spotify?: Snippet } = $props();

  let portDraft = $state(0);
  let portInput: HTMLInputElement | undefined = $state();
  let portNote: string | null = $state(null);
  let hotkeyError: string | null = $state(null);
  let recording = $state(false);
  let toast: string | null = $state(null);

  $effect(() => {
    portDraft = settings.port;
  });
  $effect(() => {
    if (focusPort && portInput) {
      portInput.focus();
      portInput.select();
    }
  });

  async function applyPort() {
    const p = Math.round(portDraft);
    if (!(p >= 1024 && p <= 65535)) {
      portNote = "Choose a port between 1024 and 65535.";
      return;
    }
    if (p === settings.port) return;
    const e = await onSave({ port: p });
    portNote = e ?? `Now on ${p}. Update the URL in your OBS Browser Source and, if connected, the Spotify redirect URI.`;
  }

  function keyName(e: KeyboardEvent): string | null {
    if (/^Key[A-Z]$/.test(e.code)) return e.code.slice(3);
    if (/^Digit\d$/.test(e.code)) return e.code.slice(5);
    if (/^F\d{1,2}$/.test(e.code)) return e.code;
    if (["Space", "Home", "End", "PageUp", "PageDown", "Insert", "Delete"].includes(e.code)) return e.code;
    return null;
  }

  async function record(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    if (e.key === "Escape") {
      recording = false;
      return;
    }
    const key = keyName(e);
    if (!key) return; // still holding modifiers
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
    if (!mods.length) {
      hotkeyError = "Use at least one modifier (Ctrl, Alt, Shift).";
      return;
    }
    recording = false;
    hotkeyError = await onSave({ hotkey: [...mods, key].join("+") });
  }

  async function run(fn: () => Promise<unknown>, done: string) {
    try {
      await fn();
      toast = done;
    } catch (e) {
      toast = String(e);
    }
    setTimeout(() => (toast = null), 2000);
  }
</script>

<div class="settings">
  <section class="card">
    <header><h2>Overlay</h2></header>
    <div class="field">
      <label for="port">Local port</label>
      <div class="inline">
        <input id="port" type="number" min="1024" max="65535" bind:value={portDraft} bind:this={portInput} onkeydown={(e) => e.key === "Enter" && applyPort()} />
        <button class="btn" onclick={applyPort} disabled={portDraft === settings.port}>Apply</button>
      </div>
      <p class="hint">{portNote ?? "OBS and the Spotify redirect use this port. Default 7878."}</p>
    </div>
    <div class="field">
      <span class="lbl">Switch-layout hotkey</span>
      <button class="btn recorder" class:recording onclick={() => { recording = true; hotkeyError = null; }} onkeydown={record} onblur={() => (recording = false)}>
        {recording ? "Press a shortcut…" : settings.hotkey}
      </button>
      {#if hotkeyError}<p class="hint err">{hotkeyError}</p>{/if}
    </div>
    <label class="switch"><span>Hide the overlay when nothing is playing</span><input type="checkbox" checked={settings.hideWhenIdle} onchange={(e) => onSave({ hideWhenIdle: e.currentTarget.checked })} /></label>
    <label class="switch"><span>Show plain lyrics when no synced version exists</span><input type="checkbox" checked={settings.showPlainLyrics} onchange={(e) => onSave({ showPlainLyrics: e.currentTarget.checked })} /></label>
    <label class="switch"><span>Floating read-along window (always on top)</span><input type="checkbox" checked={settings.floatingWindow} onchange={(e) => onSave({ floatingWindow: e.currentTarget.checked })} /></label>
  </section>

  <RomanizationSection {settings} {onSave} />

  <section class="card">
    <header><h2>App</h2></header>
    <label class="switch"><span>Start with Windows</span><input type="checkbox" checked={settings.startWithWindows} onchange={(e) => onSave({ startWithWindows: e.currentTarget.checked })} /></label>
    <label class="switch"><span>Closing the window keeps OpenMusix in the tray</span><input type="checkbox" checked={settings.minimizeToTray} onchange={(e) => onSave({ minimizeToTray: e.currentTarget.checked })} /></label>
  </section>

  <section class="card">
    <header>
      <h2>Lyrics</h2>
      <p class="hint">Lyrics come from LRCLIB. Drop your own files named <code>Artist - Title.lrc</code> in the lyrics folder to override them.</p>
    </header>
    <div class="actions">
      <button class="btn" onclick={() => run(api.openLyricsFolder, "Opened")}><span class="icon">folder_open</span>Open lyrics folder</button>
      <button class="btn" onclick={() => run(api.clearCache, "Cache cleared")}><span class="icon">delete_sweep</span>Clear lyrics cache</button>
      {#if toast}<span class="hint">{toast}</span>{/if}
    </div>
  </section>

  {@render spotify?.()}
</div>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    max-width: 640px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    margin-bottom: var(--sp-4);
  }
  label,
  .lbl,
  .switch span {
    color: var(--ink-2);
    font-size: 13px;
  }
  .inline {
    display: flex;
    gap: var(--sp-2);
  }
  .inline input {
    max-width: 140px;
    font-variant-numeric: tabular-nums;
  }
  .recorder {
    justify-content: flex-start;
    width: 220px;
    font-variant-numeric: tabular-nums;
  }
  .recorder.recording {
    box-shadow: inset 0 0 0 1.5px var(--tally);
    color: var(--tally);
  }
  .err {
    color: var(--onair);
  }
  .actions {
    display: flex;
    gap: var(--sp-2);
    align-items: center;
    flex-wrap: wrap;
  }
  code {
    color: var(--ink-2);
    font-size: 12px;
  }
</style>
