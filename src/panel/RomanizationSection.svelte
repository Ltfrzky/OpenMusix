<script lang="ts">
  import { onMount } from "svelte";
  import type { PackInfo, RomanDisplay, RomanLang, Settings } from "../lib/types";
  import { api, onPackProgress, onPacksChanged } from "./api";

  let { settings, onSave }: { settings: Settings; onSave: (patch: Partial<Settings>) => Promise<string | null> } = $props();

  let packs: PackInfo[] = $state([]);
  let errors: Partial<Record<RomanLang, string>> = $state({});

  onMount(() => {
    api.listPacks().then((p) => (packs = p)).catch(() => {});
    const offs = [
      onPackProgress(({ id, pct }) => {
        packs = packs.map((p) => (p.id === id ? { id: p.id, label: p.label, size: p.size, state: "installing", pct } : p));
      }),
      onPacksChanged((p) => (packs = p)),
    ];
    return () => offs.forEach((o) => o.then((un) => un()).catch(() => {}));
  });

  function save(patch: Partial<Settings["romanize"]>) {
    return onSave({ romanize: { ...settings.romanize, ...patch } });
  }

  async function act(id: RomanLang, fn: (id: RomanLang) => Promise<void>) {
    errors = { ...errors, [id]: undefined };
    try {
      await fn(id);
    } catch (e) {
      errors = { ...errors, [id]: String(e) };
    }
    packs = await api.listPacks().catch(() => packs);
  }

  function install(id: RomanLang) {
    packs = packs.map((p) => (p.id === id ? { id: p.id, label: p.label, size: p.size, state: "installing", pct: 0 } : p));
    return act(id, api.installPack);
  }

  const mb = (n: number) => `${(n / 1_000_000).toFixed(1)} MB`;
  const options: { value: RomanDisplay; label: string }[] = [
    { value: "below", label: "Under the original" },
    { value: "replace", label: "Instead of the original" },
  ];
</script>

<section class="card">
  <header>
    <h2>Romanization</h2>
    <p class="hint">Shows Chinese as pinyin, Japanese as romaji and Korean in Latin letters. Readings can be wrong for words with several pronunciations.</p>
  </header>
  <label class="switch">
    <span>Romanize Chinese, Japanese and Korean lyrics</span>
    <input type="checkbox" checked={settings.romanize.enabled} onchange={(e) => save({ enabled: e.currentTarget.checked })} />
  </label>
  <fieldset class="field" disabled={!settings.romanize.enabled}>
    <legend class="lbl">Show romanization:</legend>
    <div class="radios">
      {#each options as o (o.value)}
        <label class="radio">
          <input type="radio" name="roman-display" value={o.value} checked={settings.romanize.display === o.value} onchange={() => save({ display: o.value })} />
          <span>{o.label}</span>
        </label>
      {/each}
    </div>
  </fieldset>
  <div class="field">
    <span class="lbl">Language packs</span>
    <ul class="packs">
      {#each packs as p (p.id)}
        <li>
          <span class="name">{p.label}</span>
          <span class="size">{p.state === "builtIn" ? "Built-in" : mb(p.size)}</span>
          <span class="act">
            {#if p.state === "notInstalled"}
              <button class="btn" onclick={() => install(p.id)}><span class="icon">download</span>Download</button>
            {:else if p.state === "installing"}
              <span class="pct">{p.pct}%</span>
              <button class="btn ghost" onclick={() => api.cancelPackInstall(p.id)}>Cancel</button>
            {:else if p.state === "installed"}
              <button class="btn ghost" onclick={() => act(p.id, api.removePack)}>Remove</button>
            {:else if p.state === "updateAvailable"}
              <button class="btn" onclick={() => install(p.id)}>Update</button>
            {:else if p.state === "broken"}
              <button class="btn" onclick={() => install(p.id)}>Reinstall</button>
            {/if}
          </span>
          {#if errors[p.id]}<p class="hint err">{errors[p.id]}</p>{/if}
        </li>
      {/each}
    </ul>
  </div>
</section>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    margin: var(--sp-4) 0 0;
    padding: 0;
    border: 0;
  }
  .field:disabled {
    opacity: 0.5;
  }
  .lbl,
  .switch span,
  .radio span {
    color: var(--ink-2);
    font-size: 13px;
  }
  .radios {
    display: flex;
    gap: var(--sp-4);
    flex-wrap: wrap;
  }
  .radio {
    display: inline-flex;
    gap: var(--sp-2);
    align-items: center;
  }
  .packs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .packs li {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: var(--sp-3);
    align-items: center;
    min-height: 36px;
  }
  .packs .err {
    grid-column: 1 / -1;
  }
  .name {
    color: var(--ink-1);
    font-size: 13px;
  }
  .size,
  .pct {
    color: var(--ink-3);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .act {
    display: inline-flex;
    gap: var(--sp-2);
    align-items: center;
    justify-content: flex-end;
    min-width: 140px;
  }
  .err {
    color: var(--onair);
  }
</style>
