<script lang="ts">
  import Pill from "./lib/Pill.svelte";
  import Settings from "./lib/Settings.svelte";
  import Welcome from "./lib/Welcome.svelte";
  import { currentWindowLabel } from "./lib/ipc";

  // One bundle, three windows: the floating pill ("main"), the standalone
  // settings window ("settings") and the first-launch welcome ("welcome").
  // Route by the window label.
  const label = currentWindowLabel();
</script>

{#if label === "settings"}
  <main class="settings-window">
    <Settings />
  </main>
{:else if label === "welcome"}
  <main class="settings-window">
    <Welcome />
  </main>
{:else}
  <Pill />
{/if}

<style>
  .settings-window {
    height: 100%;
    background-color: #1b1b1f;
    padding: 1.5rem 1.5rem 2rem;
    overflow-y: auto;
  }
</style>
