<script lang="ts">
  import { onMount } from "svelte";
  import type { Settings } from "../lib/types";
  import { api, onSpotifyStatus, type SpotifyStatus } from "./api";

  let { settings, onSave }: { settings: Settings; onSave: (patch: Partial<Settings>) => Promise<string | null> } = $props();

  let status = $state<SpotifyStatus | null>(null);
  let clientId = $state("");
  let error = $state<string | null>(null);
  let waiting = $state(false);
  let copied = $state(false);

  const redirect = $derived(`http://127.0.0.1:${settings.port}/spotify/callback`);

  $effect(() => {
    clientId = settings.spotifyClientId ?? "";
  });

  const refresh = async () => (status = await api.spotifyStatus());
  onMount(() => {
    refresh();
    const off = onSpotifyStatus(() => {
      waiting = false;
      refresh();
    });
    return () => off.then((f) => f());
  });

  async function connect() {
    error = null;
    const trimmed = clientId.trim();
    if (!/^[0-9a-f]{32}$/i.test(trimmed)) {
      error = "A Client ID is 32 letters and numbers, from your app's Settings page.";
      return;
    }
    if (trimmed !== settings.spotifyClientId) {
      const e = await onSave({ spotifyClientId: trimmed });
      if (e) return void (error = e);
    }
    try {
      await api.spotifyConnect();
      waiting = true;
    } catch (e) {
      error = String(e);
    }
  }
  async function disconnect() {
    await api.spotifyDisconnect();
    await refresh();
  }
  async function copyRedirect() {
    await navigator.clipboard.writeText(redirect);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<section class="card">
  <header>
    <div class="title">
      <h2>Connect Spotify</h2>
      <span class="tag">Optional</span>
      <span class="tag premium">Requires Spotify Premium</span>
    </div>
    <p class="hint">OpenMusix already works with any player. Connecting Spotify sharpens timing and adds album art, even when Spotify plays on another device.</p>
  </header>

  {#if status?.connected}
    <div class="connected">
      <span class="led"></span>
      <p>Connected. The badge on the Live tab shows <strong>Spotify</strong> while its timing is in use.</p>
      <button class="btn ghost" onclick={disconnect}>Disconnect</button>
    </div>
  {:else}
    <ol>
      <li>Open <span class="mono">developer.spotify.com/dashboard</span>, sign in and choose <strong>Create app</strong>.</li>
      <li>
        Add this exact Redirect URI:
        <span class="copyrow">
          <code>{redirect}</code>
          <button class="btn" onclick={copyRedirect}><span class="icon">{copied ? "check" : "content_copy"}</span></button>
        </span>
      </li>
      <li>Under "Which API/SDKs are you planning to use?", tick <strong>Web API</strong> and save.</li>
      <li>Copy the app's <strong>Client ID</strong> and paste it here.</li>
    </ol>
    <div class="connect">
      <input type="text" placeholder="Client ID" spellcheck="false" bind:value={clientId} onkeydown={(e) => e.key === "Enter" && connect()} />
      <button class="btn tally" onclick={connect} disabled={waiting}>{waiting ? "Waiting for browser…" : "Connect"}</button>
    </div>
    {#if error}<p class="hint err">{error}</p>{/if}
    {#if waiting}<p class="hint">Approve OpenMusix in the browser tab that just opened.</p>{/if}
  {/if}
</section>

<style>
  .title {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex-wrap: wrap;
  }
  .tag {
    height: 20px;
    padding: 0 7px;
    display: inline-flex;
    align-items: center;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--ink-3);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .tag.premium {
    color: var(--tally);
    box-shadow: inset 0 0 0 1px rgba(245, 181, 68, 0.3);
  }
  ol {
    margin: 0 0 var(--sp-4);
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
    color: var(--ink-2);
    font-size: 13px;
    line-height: 1.45;
  }
  strong {
    color: var(--ink-1);
    font-weight: 600;
  }
  .mono,
  code {
    font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
    font-size: 12px;
    color: var(--ink-1);
    user-select: text;
  }
  .copyrow {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  .copyrow code {
    flex: 1;
    padding: 8px var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--booth-inset);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .copyrow .btn {
    width: 40px;
    padding: 0;
  }
  .connect {
    display: flex;
    gap: var(--sp-2);
  }
  .connect input {
    font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
  }
  .connected {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .connected p {
    flex: 1;
    color: var(--ink-2);
    font-size: 13px;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--live);
    box-shadow: 0 0 8px var(--live);
  }
  .err {
    color: var(--onair);
    margin-top: var(--sp-2);
  }
</style>
