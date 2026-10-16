<script lang="ts">
  import { onMount } from "svelte";
  import { checkPython, openPythonInstall, finishOnboarding, openSettings } from "./ipc";

  /** `undefined` while checking, `null` if missing, else the version string. */
  let python = $state<string | null | undefined>(undefined);
  let storeOpened = $state(false);

  async function recheck() {
    python = undefined;
    try {
      python = await checkPython();
    } catch (e) {
      console.error("checkPython failed", e);
      python = null;
    }
  }

  async function install() {
    await openPythonInstall();
    storeOpened = true;
  }

  async function configure() {
    await openSettings();
    await finishOnboarding();
  }

  onMount(recheck);
</script>

<div class="welcome">
  <header>
    <h1>Bienvenido a WhisperAround</h1>
    <p>Dicta en cualquier app: pulsa el atajo, habla y el texto se pega solo.</p>
  </header>

  <section class="check" class:ok={!!python} class:missing={python === null}>
    <div class="check-head">
      <span class="dot" aria-hidden="true"></span>
      <strong>Python 3</strong>
      <span class="state">
        {#if python === undefined}Comprobando…{:else if python}{python}{:else}No encontrado{/if}
      </span>
    </div>

    {#if python === null}
      <p>
        Los modelos <b>locales</b> (faster-whisper, sin internet ni API key) necesitan Python.
        Instálalo gratis desde la Microsoft Store y luego vuelve a comprobar.
      </p>
      <div class="row">
        <button class="primary" onclick={install}>Abrir Microsoft Store</button>
        <button onclick={recheck}>Volver a comprobar</button>
      </div>
      {#if storeOpened}
        <small>Cuando termine la instalación en la Store, pulsa 'Volver a comprobar'.</small>
      {/if}
    {:else if python}
      <p>Todo listo para usar los modelos locales.</p>
    {/if}
  </section>

  <p class="note">
    ¿Prefieres la nube? Los modelos de OpenAI no necesitan Python, solo tu API key en Ajustes.
  </p>

  <footer>
    <button onclick={configure}>Abrir ajustes</button>
    <button class="primary" onclick={finishOnboarding}>Continuar</button>
  </footer>
</div>

<style>
  .welcome {
    display: flex;
    flex-direction: column;
    gap: 1.1rem;
    max-width: 380px;
    margin: 0 auto;
  }
  h1 {
    margin: 0 0 0.3rem;
    font-size: 1.35rem;
  }
  header p,
  .note {
    margin: 0;
    color: #a0a0a8;
    font-size: 0.9rem;
  }
  .check {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
    padding: 0.9rem 1rem;
    border-radius: 12px;
    background: #232329;
    border: 1px solid #3a3a42;
  }
  .check.ok {
    border-color: rgba(74, 222, 128, 0.45);
  }
  .check.missing {
    border-color: rgba(251, 191, 36, 0.5);
  }
  .check p {
    margin: 0;
    font-size: 0.88rem;
    color: #cfcfd6;
  }
  .check small {
    color: #8a8a92;
    font-size: 0.78rem;
  }
  .check-head {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }
  .state {
    margin-left: auto;
    color: #a0a0a8;
    font-size: 0.85rem;
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: #6b6b74;
  }
  .ok .dot {
    background: #4ade80;
  }
  .missing .dot {
    background: #fbbf24;
  }
  .row,
  footer {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  footer {
    justify-content: flex-end;
    margin-top: 0.4rem;
  }
  .primary {
    background-color: #646cff;
    color: #fff;
  }
  .primary:hover {
    background-color: #7a80ff;
  }
</style>
