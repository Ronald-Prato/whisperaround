<script lang="ts">
  import { onMount } from "svelte";
  import {
    checkPython,
    openPythonInstall,
    finishOnboarding,
    openSettings,
    checkPermissions,
    requestPermission,
    IS_MAC,
  } from "./ipc";
  import type { Permissions } from "./types";

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

  /** macOS only (null elsewhere); polled so toggles made in System Settings show up. */
  let perms = $state<Permissions | null>(null);

  async function refreshPermissions() {
    try {
      perms = await checkPermissions();
    } catch (e) {
      console.error("checkPermissions failed", e);
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

  onMount(() => {
    recheck();
    refreshPermissions();
    const timer = setInterval(refreshPermissions, 1500);
    return () => clearInterval(timer);
  });
</script>

<div class="welcome">
  <header>
    <h1>Bienvenido a WhisperAround</h1>
    <p>Dicta en cualquier app: pulsa el atajo, habla y el texto se pega solo.</p>
  </header>

  {#if perms}
    <section class="check" class:ok={perms.microphone === "granted"} class:missing={perms.microphone === "denied"}>
      <div class="check-head">
        <span class="dot" aria-hidden="true"></span>
        <strong>Micrófono</strong>
        <span class="state">
          {#if perms.microphone === "granted"}Permitido{:else if perms.microphone === "denied"}Bloqueado{:else}Sin pedir{/if}
        </span>
      </div>
      {#if perms.microphone !== "granted"}
        <p>Para grabar tu voz. macOS te lo pedirá una vez.</p>
        <div class="row">
          <button class="primary" onclick={() => requestPermission("microphone")}>
            {perms.microphone === "denied" ? "Abrir Ajustes del Sistema" : "Permitir micrófono"}
          </button>
        </div>
      {/if}
    </section>

    <section class="check" class:ok={perms.accessibility} class:missing={!perms.accessibility}>
      <div class="check-head">
        <span class="dot" aria-hidden="true"></span>
        <strong>Accesibilidad</strong>
        <span class="state">{perms.accessibility ? "Permitido" : "Falta"}</span>
      </div>
      {#if !perms.accessibility}
        <p>
          Para pegar el texto solo (⌘V) en la app que uses. Activa <b>WhisperAround</b> en
          Ajustes del Sistema › Privacidad y seguridad › Accesibilidad. Sin esto, el texto queda en
          el portapapeles.
        </p>
        <div class="row">
          <button class="primary" onclick={() => requestPermission("accessibility")}>Permitir Accesibilidad</button>
        </div>
      {/if}
    </section>
  {/if}

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
        {#if IS_MAC}
          Instálalo desde python.org (o con <code>brew install python</code>) y luego vuelve a comprobar.
        {:else}
          Instálalo gratis desde la Microsoft Store y luego vuelve a comprobar.
        {/if}
      </p>
      <div class="row">
        <button class="primary" onclick={install}>{IS_MAC ? "Abrir python.org" : "Abrir Microsoft Store"}</button>
        <button onclick={recheck}>Volver a comprobar</button>
      </div>
      {#if storeOpened}
        <small>Cuando termine la instalación, pulsa 'Volver a comprobar'.</small>
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
