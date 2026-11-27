<script lang="ts">
  import { onMount } from "svelte";
  import type { Mode, Snapshot } from "../lib/types";
  import { connectSnapshots } from "../lib/ws";
  import Overlay from "./Overlay.svelte";

  let { wsUrl, artBase, pinnedMode, float }: { wsUrl: string; artBase: string; pinnedMode?: Mode; float: boolean } = $props();
  let snapshot: Snapshot | null = $state(null);

  onMount(() => connectSnapshots(wsUrl, (s) => (snapshot = s)));
</script>

<div class="stage" data-tauri-drag-region={float ? "" : undefined}>
  <Overlay {snapshot} {pinnedMode} {artBase} />
</div>

<style>
  .stage {
    position: fixed;
    inset: 0;
  }
</style>
