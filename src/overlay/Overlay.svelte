<script lang="ts">
  import type { LyricLine, Mode, Snapshot } from "../lib/types";
  import { PlaybackClock, effectivePosition } from "../lib/timing";
  import { currentLineIndex } from "../lib/lyricsIndex";
  import { cssText, styleVars } from "./styleVars";
  import FocusView from "./FocusView.svelte";
  import WidgetView from "./WidgetView.svelte";

  let {
    snapshot,
    pinnedMode,
    preview = false,
    artBase = "",
    fit = false,
  }: { snapshot: Snapshot | null; pinnedMode?: Mode; preview?: boolean; artBase?: string; fit?: boolean } = $props();

  const clock = new PlaybackClock();
  let index = $state(-1);

  $effect.pre(() => {
    const np = snapshot?.nowPlaying;
    if (np) {
      clock.update(
        { positionMs: np.positionMs, sampledAt: np.sampledAt, isPlaying: np.isPlaying, trackKey: snapshot?.trackKey ?? null },
        Date.now(),
      );
    }
  });

  const mode: Mode = $derived(pinnedMode ?? snapshot?.mode ?? "focus");
  const style = $derived(snapshot ? snapshot.styles[mode] : null);
  const np = $derived(snapshot?.nowPlaying ?? null);
  const result = $derived(snapshot?.lyrics.status === "ready" ? snapshot.lyrics.result : null);
  const lines: LyricLine[] = $derived(result?.kind === "synced" ? result.value : []);

  /** One short status line under the song info, or null when lyrics are on screen. */
  const message = $derived.by(() => {
    if (!result) return null;
    if (result.kind === "plainOnly") return "No synced lyrics";
    if (result.kind === "instrumental") return "♪ Instrumental";
    return null;
  });
  const plainText = $derived(result?.kind === "plainOnly" && snapshot?.showPlainLyrics ? result.value : null);
  const artUrl = $derived(np?.track.artUrl ? (np.track.artUrl.startsWith("/") ? artBase + np.track.artUrl : np.track.artUrl) : null);

  const raf: (cb: () => void) => number =
    typeof requestAnimationFrame === "function" ? requestAnimationFrame : (cb) => setTimeout(cb, 16) as unknown as number;
  const caf: (id: number) => void = typeof cancelAnimationFrame === "function" ? cancelAnimationFrame : (id) => clearTimeout(id);

  $effect(() => {
    const s = snapshot;
    const ls = lines;
    let id = 0;
    const loop = () => {
      index = s && ls.length ? currentLineIndex(ls, effectivePosition(clock, Date.now(), s.globalOffsetMs, s.trackOffsetMs)) : -1;
      id = raf(loop);
    };
    loop();
    return () => caf(id);
  });
</script>

{#if snapshot && style && !(np === null && snapshot.hideWhenIdle)}
  <div
    class="om-root"
    class:idle={np === null}
    class:paused={np !== null && !np.isPlaying}
    class:preview
    style={cssText(styleVars(style, { fit }))}
  >
    {#if np}
      {#if mode === "focus"}
        <FocusView {lines} {index} {style} track={np.track} {message} {plainText} />
      {:else}
        <WidgetView {lines} {index} {style} track={np.track} {message} {plainText} {artUrl} />
      {/if}
    {/if}
  </div>
{/if}

<style>
  .om-root {
    position: absolute;
    inset: 0;
    overflow: hidden;
    font-family: var(--om-font);
    color: var(--om-text);
    -webkit-font-smoothing: antialiased;
    transition-property: opacity;
    transition-duration: 400ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
  .om-root.paused {
    opacity: 0.45;
  }
  .om-root.idle {
    opacity: 0;
    transition-duration: 600ms;
  }
</style>
