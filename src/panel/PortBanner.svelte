<script lang="ts">
  import type { ServerStatus } from "../lib/types";

  let { status, onFix, onRetry }: { status: ServerStatus | null; onFix: () => void; onRetry: () => void } = $props();
</script>

{#if status && status.state !== "running"}
  <div class="banner" role="alert">
    <span class="icon">warning</span>
    <p>
      {#if status.state === "portInUse"}
        <strong>Port {status.port} is in use by another program.</strong> The overlay is offline until the port is free or changed.
      {:else}
        <strong>The overlay server could not start.</strong> {status.message}
      {/if}
    </p>
    <button class="btn ghost" onclick={onRetry}>Retry</button>
    <button class="btn tally" onclick={onFix}>Change port</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    padding: var(--sp-3) var(--sp-4);
    border-radius: var(--r-md);
    background: var(--warn-bg);
    box-shadow: inset 0 0 0 1px rgba(229, 72, 77, 0.3);
  }
  .icon {
    color: var(--onair);
  }
  p {
    flex: 1;
    color: var(--ink-2);
    font-size: 13px;
  }
  strong {
    color: var(--ink-1);
    font-weight: 600;
  }
</style>
