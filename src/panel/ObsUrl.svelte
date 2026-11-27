<script lang="ts">
  import type { Mode } from "../lib/types";
  import { obsUrl } from "./obsUrl";

  let { port }: { port: number } = $props();
  let copied: string | null = $state(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const rows: { id: string; mode?: Mode; label: string; size: string }[] = [
    { id: "auto", label: "Follows the switch", size: "1920 × 1080" },
    { id: "focus", mode: "focus", label: "Always Focus", size: "1920 × 1080" },
    { id: "widget", mode: "widget", label: "Always Widget", size: "600 × 200" },
  ];

  async function copy(id: string, url: string) {
    await navigator.clipboard.writeText(url);
    copied = id;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = null), 1600);
  }
</script>

<section class="card">
  <header>
    <h2>Add to OBS</h2>
    <p class="hint">Sources → + → Browser, paste the URL, set the size.</p>
  </header>
  <ul>
    {#each rows as r (r.id)}
      {@const url = obsUrl(port, r.mode)}
      <li>
        <div class="txt">
          <span class="label">{r.label}</span>
          <span class="url">{url.replace("http://", "")}</span>
        </div>
        <span class="size num">{r.size}</span>
        <button class="btn" onclick={() => copy(r.id, url)} aria-label="Copy {r.label} URL">
          <span class="swap">
            <span class="icon" class:shown={copied !== r.id}>content_copy</span>
            <span class="icon ok" class:shown={copied === r.id}>check</span>
          </span>
        </button>
      </li>
    {/each}
  </ul>
</section>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
  }
  li + li {
    box-shadow: inset 0 1px 0 var(--rule-soft);
  }
  .txt {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: 13px;
  }
  .url {
    font-size: 12px;
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    user-select: text;
  }
  .size {
    color: var(--ink-3);
    font-size: 12px;
  }
  .btn {
    width: 40px;
    padding: 0;
  }
  .swap {
    position: relative;
    width: 18px;
    height: 18px;
  }
  .swap .icon {
    position: absolute;
    inset: 0;
    opacity: 0;
    scale: 0.25;
    filter: blur(4px);
    transition-property: opacity, scale, filter;
    transition-duration: 200ms;
    transition-timing-function: cubic-bezier(0.2, 0, 0, 1);
  }
  .swap .icon.shown {
    opacity: 1;
    scale: 1;
    filter: blur(0);
  }
  .ok {
    color: var(--live);
  }
</style>
