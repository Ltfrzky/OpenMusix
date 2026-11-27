<script lang="ts">
  import { onMount } from "svelte";
  import type { Mode, Snapshot } from "../lib/types";
  import { connectSnapshots } from "../lib/ws";
  import Overlay from "./Overlay.svelte";

  let { wsUrl, artBase, pinnedMode, float }: { wsUrl: string; artBase: string; pinnedMode?: Mode; float: boolean } = $props();
  let snapshot: Snapshot | null = $state(null);

  onMount(() => connectSnapshots(wsUrl, (s) => (snapshot = s)));

  // Tauri's drag-region attribute only fires when the clicked element itself carries it, and the
  // lyrics cover the whole stage, so start the drag by hand from anywhere in the floating window.
  async function drag(e: MouseEvent) {
    if (!float || e.button !== 0) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().startDragging();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="stage" class:float onmousedown={drag}>
  <Overlay {snapshot} {pinnedMode} {artBase} fit={float} />
</div>

<style>
  .stage {
    position: fixed;
    inset: 0;
  }
  .stage.float {
    cursor: grab;
    user-select: none;
  }
  .stage.float :global(*) {
    cursor: inherit;
  }
</style>
