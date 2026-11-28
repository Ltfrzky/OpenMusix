<script lang="ts">
  import type { RomanDisplay } from "../lib/types";

  /** One lyric line with its romanization: under it, or in its place. */
  let { text, roman, display }: { text: string; roman: string | null; display: RomanDisplay } = $props();
</script>

{#if roman && display === "replace"}{roman}{:else}{text}{#if roman}<span class="om-roman">{roman}</span>{/if}{/if}

<style>
  .om-roman {
    display: block;
    font-size: 0.55em;
    font-weight: 500;
    opacity: 0.7;
  }
</style>
