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
    roman = null,
    display = "below",
  }: {
    lines: LyricLine[];
    index: number;
    style: Style;
    track: Track;
    message: string | null;
    plainText: string | null;
    roman?: (string | null)[] | null;
    display?: RomanDisplay;
  } = $props();

  const plainRows = $derived(plainText ? plainText.split(/\r?\n/) : []);

  let els: HTMLElement[] = $state([]);
  let offset = $state(0);
  const anchor = $derived(Math.max(0, index));
  const half = $derived(Math.floor(Math.max(1, style.lineCount) / 2));

  // Center the active line: translate the column so its middle sits at 50%.
  $effect(() => {
    const el = els[anchor];
    if (el) offset = -(el.offsetTop + el.offsetHeight / 2);
  });

  function look(i: number): string {
    const d = Math.abs(i - anchor);
    if (index < 0 && i === 0) return "transform: scale(0.82); opacity: 0.55";
    if (d > half) return "transform: scale(0.6); opacity: 0";
    const [s, o] = d === 0 ? [1, 1] : d === 1 ? [0.82, 0.55] : d === 2 ? [0.7, 0.35] : [0.6, 0.2];
    return `transform: scale(${s}); opacity: ${o}`;
  }
</script>

<div class="om-focus" class:panel={style.background === "panel"}>
  {#if lines.length}
    <div class="column" style="transform: translateY({offset}px)">
      {#each lines as line, i (i)}
        <p bind:this={els[i]} class="om-line" class:current={i === index} style={look(i)}>
          <RomanText text={line.text || "♪"} roman={roman?.[i] ?? null} {display} />
        </p>
      {/each}
    </div>
  {:else}
    <div class="info">
      <p class="title">{track.title}</p>
      <p class="artist">{track.artist}</p>
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
  .om-focus {
    position: absolute;
    inset: 0;
    text-align: var(--om-align);
    text-shadow: var(--om-shadow);
    -webkit-mask-image: linear-gradient(transparent, #000 14%, #000 86%, transparent);
    mask-image: linear-gradient(transparent, #000 14%, #000 86%, transparent);
  }
  .om-focus.panel {
    inset: 4%;
    border-radius: 28px;
    background: rgba(12, 12, 16, 0.62);
    backdrop-filter: blur(18px);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.06), 0 24px 60px rgba(0, 0, 0, 0.35);
  }
  .column {
    position: absolute;
    top: 50%;
    left: 6%;
    right: 6%;
    transition-property: transform;
    transition-duration: 450ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
    will-change: transform;
  }
  .om-line {
    margin: 0;
    padding: 0.14em 0;
    font-size: var(--om-size);
    font-weight: 700;
    line-height: 1.22;
    letter-spacing: -0.01em;
    color: var(--om-dim);
    text-wrap: balance;
    transform-origin: var(--om-origin);
    transition-property: transform, opacity, color;
    transition-duration: 350ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
  .plain-line {
    display: block;
  }
  .om-line.current {
    color: var(--om-hl);
  }
  .info {
    position: absolute;
    top: 50%;
    left: 6%;
    right: 6%;
    transform: translateY(-50%);
  }
  .info p {
    margin: 0;
    text-wrap: balance;
  }
  .title {
    font-size: calc(var(--om-size) * 0.8);
    font-weight: 700;
    color: var(--om-hl);
  }
  .artist {
    margin-top: 0.2em !important;
    font-size: calc(var(--om-size) * 0.5);
    color: var(--om-dim);
  }
  .message {
    margin-top: 0.9em !important;
    font-size: calc(var(--om-size) * 0.45);
    color: var(--om-dim);
  }
  .plain {
    margin-top: 1em !important;
    font-size: calc(var(--om-size) * 0.4);
    white-space: pre-line;
    color: var(--om-text);
    text-wrap: pretty;
  }
</style>
