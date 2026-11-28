<script lang="ts">
  import type { LyricLine, RomanDisplay, Style, Track } from "../lib/types";
  import RomanText from "./RomanText.svelte";

  let {
    lines,
    index,
    style,
    track,
    message,
    plainText,
    artUrl,
    roman = null,
    display = "below",
  }: {
    lines: LyricLine[];
    index: number;
    style: Style;
    track: Track;
    message: string | null;
    plainText: string | null;
    artUrl: string | null;
    roman?: (string | null)[] | null;
    display?: RomanDisplay;
  } = $props();

  const plainRows = $derived(plainText ? plainText.split(/\r?\n/) : []);

  const start = $derived(Math.max(0, index));
  const visible = $derived(lines.slice(start, start + Math.max(1, style.lineCount)));
</script>

<div class="om-widget" class:panel={style.background === "panel"}>
  <div class="head">
    {#if style.showArt && artUrl}
      <img class="art" src={artUrl} alt="" />
    {/if}
    <div class="meta">
      <p class="title">{track.title}</p>
      <p class="artist">{track.artist}</p>
    </div>
  </div>
  {#if visible.length}
    <div class="lines">
      <!-- Keyed by absolute line index: lines already on screen stay put, only the new one enters. -->
      {#each visible as line, i (start + i)}
        <p class="om-line" class:current={i === 0 && index >= 0}>
          <RomanText text={line.text || "♪"} roman={roman?.[start + i] ?? null} {display} />
        </p>
      {/each}
    </div>
  {:else if message || plainText}
    <div class="lines">
      {#if message}<p class="message">{message}</p>{/if}
      {#if plainText && roman}
        <p class="plain">
          {#each plainRows as row, i (i)}
            <span class="plain-line"><RomanText text={row} roman={roman[i] ?? null} {display} /></span>
          {/each}
        </p>
      {:else if plainText}<p class="plain">{plainText}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .plain-line {
    display: block;
  }
  .om-widget {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 12px;
    padding: 14px 18px 16px;
    text-align: var(--om-align);
    text-shadow: var(--om-shadow);
  }
  .om-widget.panel {
    border-radius: 24px;
    background: rgba(12, 12, 16, 0.72);
    backdrop-filter: blur(18px);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.07), 0 12px 32px rgba(0, 0, 0, 0.35);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .art {
    width: 48px;
    height: 48px;
    border-radius: 10px;
    object-fit: cover;
    outline: 1px solid rgba(255, 255, 255, 0.1);
    outline-offset: -1px;
    flex: none;
  }
  .meta {
    min-width: 0;
    flex: 1;
  }
  .meta p {
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .title {
    font-weight: 700;
    font-size: calc(var(--om-size) * 0.72);
    color: var(--om-hl);
  }
  .artist {
    font-size: calc(var(--om-size) * 0.6);
    color: var(--om-dim);
  }
  .lines {
    margin-top: 10px;
  }
  .om-line {
    margin: 0;
    padding: 2px 0;
    font-size: var(--om-size);
    font-weight: 650;
    line-height: 1.3;
    color: var(--om-dim);
    text-wrap: balance;
    animation: enter 320ms cubic-bezier(0.2, 0, 0, 1) both;
  }
  .om-line.current {
    color: var(--om-hl);
  }
  .message {
    margin: 0;
    font-size: calc(var(--om-size) * 0.7);
    color: var(--om-dim);
  }
  .plain {
    margin: 6px 0 0;
    max-height: 6.5em;
    overflow: hidden;
    white-space: pre-line;
    font-size: calc(var(--om-size) * 0.7);
  }
  @keyframes enter {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
</style>
