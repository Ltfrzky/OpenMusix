<script lang="ts">
  import { onMount } from "svelte";
  import type { Align, Background, Mode, Style, TextEffect } from "../lib/types";
  import { BUNDLED_FONTS, PRESETS } from "../lib/presets";
  import { api } from "./api";

  let { mode, style, onChange }: { mode: Mode; style: Style; onChange: (s: Style) => void } = $props();

  let systemFonts: string[] = $state([]);
  onMount(async () => {
    try {
      systemFonts = (await api.systemFonts()).filter((f) => !BUNDLED_FONTS.includes(f));
    } catch {
      systemFonts = [];
    }
  });

  const set = <K extends keyof Style>(k: K, v: Style[K]) => onChange({ ...style, [k]: v, preset: k === "preset" ? (v as string) : "custom" });
  const applyPreset = (id: keyof typeof PRESETS) => onChange({ ...PRESETS[id] });
  const presetNames: Record<keyof typeof PRESETS, string> = { spotlight: "Spotlight", glass: "Glass card", neon: "Neon" };
  const effects: { id: TextEffect; label: string }[] = [
    { id: "none", label: "None" },
    { id: "shadow", label: "Shadow" },
    { id: "outline", label: "Outline" },
  ];
  const aligns: { id: Align; icon: string }[] = [
    { id: "left", icon: "format_align_left" },
    { id: "center", icon: "format_align_center" },
    { id: "right", icon: "format_align_right" },
  ];
  const backgrounds: { id: Background; label: string }[] = [
    { id: "transparent", label: "Transparent" },
    { id: "panel", label: "Panel" },
  ];
</script>

<section class="card">
  <header>
    <h2>Look · {mode === "focus" ? "Focus" : "Widget"}</h2>
    <p class="hint">Changes apply to the live overlay right away.</p>
  </header>

  <div class="presets">
    {#each Object.keys(PRESETS) as id (id)}
      {@const p = PRESETS[id as keyof typeof PRESETS]}
      <button class="preset" class:active={style.preset === id} onclick={() => applyPreset(id as keyof typeof PRESETS)}>
        <span class="swatch" style="font-family: '{p.fontFamily} Variable'; color: {p.highlightColor}; background: {p.background === 'panel' ? 'rgba(12,12,16,.9)' : 'transparent'}">Aa</span>
        {presetNames[id as keyof typeof PRESETS]}
      </button>
    {/each}
  </div>

  <div class="grid">
    <label class="field wide">
      <span>Font</span>
      <select value={style.fontFamily} onchange={(e) => set("fontFamily", e.currentTarget.value)}>
        <optgroup label="Bundled">
          {#each BUNDLED_FONTS as f (f)}<option value={f}>{f}</option>{/each}
        </optgroup>
        {#if systemFonts.length}
          <optgroup label="Installed on this PC">
            {#each systemFonts as f (f)}<option value={f}>{f}</option>{/each}
          </optgroup>
        {/if}
      </select>
    </label>

    <label class="field wide">
      <span>Size <output class="num">{style.fontSizePx}px</output></span>
      <input type="range" min="12" max="120" value={style.fontSizePx} oninput={(e) => set("fontSizePx", Number(e.currentTarget.value))} />
    </label>

    <label class="field wide">
      <span>Lines shown <output class="num">{style.lineCount}</output></span>
      <input type="range" min="1" max="9" value={style.lineCount} oninput={(e) => set("lineCount", Number(e.currentTarget.value))} />
    </label>

    <div class="colors wide">
      <label class="color"><input type="color" value={style.highlightColor} oninput={(e) => set("highlightColor", e.currentTarget.value)} /><span>Current line</span></label>
      <label class="color"><input type="color" value={style.textColor} oninput={(e) => set("textColor", e.currentTarget.value)} /><span>Text</span></label>
      <label class="color"><input type="color" value={toHex(style.dimColor)} oninput={(e) => set("dimColor", withAlpha(e.currentTarget.value, 0.45))} /><span>Other lines</span></label>
    </div>

    <div class="field wide">
      <span>Text effect</span>
      <div class="seg">
        {#each effects as e (e.id)}
          <button class:active={style.effect === e.id} onclick={() => set("effect", e.id)}>{e.label}</button>
        {/each}
      </div>
    </div>

    <div class="field">
      <span>Align</span>
      <div class="seg">
        {#each aligns as a (a.id)}
          <button class:active={style.align === a.id} onclick={() => set("align", a.id)} aria-label="Align {a.id}"><span class="icon">{a.icon}</span></button>
        {/each}
      </div>
    </div>

    <div class="field">
      <span>Background</span>
      <div class="seg">
        {#each backgrounds as b (b.id)}
          <button class:active={style.background === b.id} onclick={() => set("background", b.id)}>{b.label}</button>
        {/each}
      </div>
    </div>

    <label class="switch wide">
      <span>Show album art</span>
      <input type="checkbox" checked={style.showArt} onchange={(e) => set("showArt", e.currentTarget.checked)} />
    </label>
  </div>
</section>

<script lang="ts" module>
  /** Best-effort conversion of `#rgb`, `#rrggbb` or `rgba(r,g,b,a)` to `#rrggbb` for <input type=color>. */
  export function toHex(c: string): string {
    if (/^#[0-9a-f]{6}$/i.test(c)) return c;
    if (/^#[0-9a-f]{3}$/i.test(c)) return "#" + [...c.slice(1)].map((x) => x + x).join("");
    const m = c.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i);
    if (!m) return "#ffffff";
    return "#" + m.slice(1, 4).map((n) => Number(n).toString(16).padStart(2, "0")).join("");
  }
  export function withAlpha(hex: string, a: number): string {
    const n = parseInt(hex.slice(1), 16);
    return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
  }
</script>

<style>
  .presets {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--sp-2);
    margin-bottom: var(--sp-4);
  }
  .preset {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: var(--sp-2) var(--sp-2) var(--sp-3);
    border: 0;
    border-radius: var(--r-md);
    background: var(--booth-inset);
    box-shadow: inset 0 0 0 1px var(--rule);
    color: var(--ink-2);
    font: 500 12px var(--font-ui);
    cursor: pointer;
    transition-property: box-shadow, scale;
    transition-duration: 150ms;
  }
  .preset:active {
    scale: 0.96;
  }
  .preset.active {
    box-shadow: inset 0 0 0 1.5px var(--tally);
    color: var(--ink-1);
  }
  .swatch {
    width: 100%;
    height: 40px;
    display: grid;
    place-items: center;
    border-radius: 4px;
    font-size: 20px;
    font-weight: 700;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-4) var(--sp-3);
  }
  .wide {
    grid-column: 1 / -1;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .field > span {
    display: flex;
    justify-content: space-between;
    color: var(--ink-2);
    font-size: 13px;
  }
  .colors {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--sp-2);
  }
  .color {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-size: 12px;
    color: var(--ink-2);
    cursor: pointer;
  }
  .seg {
    display: flex;
    padding: 3px;
    border-radius: 11px;
    background: var(--booth-inset);
    box-shadow: inset 0 0 0 1px var(--rule);
  }
  .seg button {
    flex: 1;
    min-height: 32px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-3);
    font: 500 12px var(--font-ui);
    cursor: pointer;
    display: grid;
    place-items: center;
    transition-property: background-color, color;
    transition-duration: 150ms;
  }
  .seg button.active {
    background: var(--booth-high);
    color: var(--ink-1);
    box-shadow: inset 0 0 0 1px var(--rule-strong);
  }
  .switch span {
    color: var(--ink-2);
    font-size: 13px;
  }
</style>
