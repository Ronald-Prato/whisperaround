<script lang="ts">
  import { onMount } from "svelte";
  import { loadSettings, saveSettings, appVersion, checkUpdate, installUpdate } from "./ipc";
  import { MODELS } from "./models";
  import type { SettingsView } from "./types";

  let model = $state("gpt-4o-mini-transcribe");
  let shortcut = $state("CmdOrCtrl+Shift+X");
  let autoPaste = $state(true);
  let apiKeyConfigured = $state(false);
  /** Empty means "leave the stored key untouched". */
  let apiKeyInput = $state("");

  let saving = $state(false);
  let savedAt = $state<number | null>(null);
  let saveError = $state<string | null>(null);

  let version = $state("");
  /** Newer release on GitHub, if any. */
  let update = $state<string | null>(null);
  let updating = $state(false);
  let updateError = $state<string | null>(null);

  async function runUpdate() {
    updating = true;
    updateError = null;
    try {
      await installUpdate(); // restarts the app on success
    } catch (e) {
      updateError = String(e);
      updating = false;
    }
  }

  function apply(view: SettingsView) {
    model = view.model;
    shortcut = view.shortcut;
    autoPaste = view.autoPaste;
    apiKeyConfigured = view.apiKeyConfigured;
    apiKeyInput = "";
  }

  onMount(() => {
    loadSettings().then(apply).catch((e) => console.error("loadSettings failed", e));
    appVersion().then((v) => (version = v));
    checkUpdate()
      .then((v) => (update = v))
      .catch((e) => console.error("checkUpdate failed", e));
  });

  async function save() {
    saving = true;
    saveError = null;
    try {
      const view = await saveSettings({
        openaiApiKey: apiKeyInput,
        model,
        shortcut,
        autoPaste,
      });
      apply(view);
      savedAt = Date.now();
    } catch (e) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<form class="settings" onsubmit={(e) => (e.preventDefault(), save())}>
  {#if update}
    <div class="update">
      <span>Hay una versión nueva: <b>{update}</b></span>
      <button type="button" class="primary" disabled={updating} onclick={runUpdate}>
        {updating ? "Actualizando…" : "Actualizar"}
      </button>
    </div>
    {#if updateError}<small class="err">{updateError}</small>{/if}
  {/if}

  <label class="field">
    <span>OpenAI API key</span>
    <input
      type="password"
      autocomplete="off"
      placeholder={apiKeyConfigured ? "•••••••••• (configurada)" : "sk-…"}
      bind:value={apiKeyInput}
    />
    <small>
      {apiKeyConfigured
        ? "Hay una key guardada. Déjalo en blanco para conservarla."
        : "Aún no hay key configurada."}
      Se guarda en el llavero del sistema y solo se envía a api.openai.com.
    </small>
  </label>

  <label class="field">
    <span>Modelo</span>
    <select bind:value={model}>
      <optgroup label="OpenAI (nube)">
        {#each MODELS.filter((m) => !m.local) as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
      </optgroup>
      <optgroup label="Local (faster-whisper)">
        {#each MODELS.filter((m) => m.local) as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
      </optgroup>
    </select>
  </label>

  <label class="field">
    <span>Atajo global</span>
    <input type="text" bind:value={shortcut} placeholder="CmdOrCtrl+Shift+X" />
    <small>Formato Tauri, p. ej. <code>CmdOrCtrl+Shift+X</code>. Se aplica al guardar (sin reiniciar).</small>
  </label>

  <label class="field checkbox">
    <input type="checkbox" bind:checked={autoPaste} />
    <span>Pegar automáticamente (Ctrl/Cmd+V) tras transcribir</span>
  </label>

  <div class="actions">
    <button type="submit" disabled={saving}>
      {saving ? "Guardando…" : "Guardar"}
    </button>
    {#if saveError}
      <span class="err">{saveError}</span>
    {:else if savedAt}
      <span class="saved">Guardado ✓</span>
    {/if}
  </div>

  {#if version}<small class="version">WhisperAround {version}</small>{/if}
</form>

<style>
  .settings {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 360px;
    margin: 0 auto;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .field > span {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .field small {
    color: #8a8a92;
    font-size: 0.78rem;
  }

  .field.checkbox {
    flex-direction: row;
    align-items: center;
    gap: 0.6rem;
  }

  .field.checkbox input {
    width: auto;
  }

  .field.checkbox span {
    font-weight: 400;
    font-size: 0.9rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .saved {
    color: #4ade80;
    font-size: 0.85rem;
  }

  .err {
    color: #f87171;
    font-size: 0.82rem;
  }

  .update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8rem;
    padding: 0.6rem 0.8rem;
    border-radius: 10px;
    background: rgba(100, 108, 255, 0.12);
    border: 1px solid rgba(100, 108, 255, 0.45);
    font-size: 0.88rem;
  }

  .primary {
    background-color: #646cff;
    color: #fff;
  }

  .version {
    color: #6b6b74;
    font-size: 0.75rem;
    text-align: center;
  }

  code {
    background-color: #232329;
    padding: 0 0.3em;
    border-radius: 4px;
  }
</style>
