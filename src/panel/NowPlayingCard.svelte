<script lang="ts">
  import type { RomanLang, Snapshot } from "../lib/types";
  import { api } from "./api";

  let { snapshot, artBase }: { snapshot: Snapshot | null; artBase: string } = $props();

  const LANG: Record<RomanLang, string> = { zh: "Chinese", ja: "Japanese", ko: "Korean" };
  const missing = $derived(snapshot?.nowPlaying && snapshot.romanize.enabled ? snapshot.romanMissing : []);
  let installing: RomanLang | null = $state(null);
  async function install(id: RomanLang) {
    installing = id;
    try {
      await api.installPack(id);
    } finally {
      installing = null;
    }
  }

  const np = $derived(snapshot?.nowPlaying ?? null);
  const art = $derived(np?.track.artUrl ? (np.track.artUrl.startsWith("/") ? artBase + np.track.artUrl : np.track.artUrl) : null);

  const lyricsChip = $derived.by(() => {
    const l = snapshot?.lyrics;
    if (!np || !l) return null;
    if (l.status === "loading") return { label: "Searching lyrics", tone: "muted" };
    if (l.status === "error") return { label: "Lyrics offline", tone: "warn" };
    switch (l.result.kind) {
      case "synced": return { label: "Synced lyrics", tone: "live" };
      case "plainOnly": return { label: "Plain lyrics only", tone: "muted" };
      case "instrumental": return { label: "Instrumental", tone: "muted" };
      default: return { label: "No lyrics found", tone: "muted" };
    }
  });
</script>

<section class="np" class:idle={!np}>
  <div class="art">
    {#if art}
      <img src={art} alt="" />
    {:else}
      <span class="icon">music_note</span>
    {/if}
  </div>
  <div class="meta">
    <div class="tags">
      <span class="tally" class:on={np?.isPlaying}>
        <span class="dot"></span>{np ? (np.isPlaying ? "On air" : "Paused") : "Standing by"}
      </span>
      {#if np}
        <span class="source">{np.source === "spotify" ? "Spotify" : "Windows"}</span>
      {/if}
      {#if lyricsChip}
        <span class="chip {lyricsChip.tone}">{lyricsChip.label}</span>
      {/if}
    </div>
    {#if np}
      <h1 class="title">{np.track.title}</h1>
      <p class="artist">{np.track.artist}{np.track.album ? ` · ${np.track.album}` : ""}</p>
      {#each missing as id (id)}
        <p class="roman-hint">
          This song is in {LANG[id]}. Install the {LANG[id]} pack to romanize it.
          <button class="btn" disabled={installing !== null} onclick={() => install(id)}>Install</button>
        </p>
      {/each}
    {:else}
      <h1 class="title muted">Nothing playing</h1>
      <p class="artist">Play a song in Spotify, YouTube Music or any player — it shows up here.</p>
    {/if}
  </div>
</section>

<style>
  .np {
    display: flex;
    gap: var(--sp-4);
    align-items: center;
    min-width: 0;
  }
  .art {
    flex: none;
    width: 72px;
    height: 72px;
    border-radius: var(--r-md);
    background: var(--booth-high);
    display: grid;
    place-items: center;
    overflow: hidden;
    color: var(--ink-3);
    outline: 1px solid rgba(255, 255, 255, 0.1);
    outline-offset: -1px;
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .art .icon {
    font-size: 28px;
  }
  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tags {
    display: flex;
    gap: var(--sp-2);
    align-items: center;
    flex-wrap: wrap;
    margin-bottom: 2px;
  }
  .tally,
  .source,
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 8px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-3);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .tally .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--ink-4);
  }
  .tally.on {
    color: var(--onair);
    box-shadow: inset 0 0 0 1px rgba(229, 72, 77, 0.35);
  }
  .tally.on .dot {
    background: var(--onair);
    box-shadow: 0 0 8px var(--onair);
  }
  .source {
    color: var(--ink-2);
  }
  .chip.live {
    color: var(--live);
  }
  .chip.warn {
    color: var(--onair);
  }
  .title {
    font-size: 22px;
    line-height: 1.2;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title.muted {
    color: var(--ink-2);
  }
  .artist {
    color: var(--ink-2);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .roman-hint {
    display: flex;
    gap: var(--sp-2);
    align-items: center;
    flex-wrap: wrap;
    margin-top: 4px;
    color: var(--ink-3);
    font-size: 12px;
  }
  .idle .artist {
    white-space: normal;
    color: var(--ink-3);
  }
</style>
