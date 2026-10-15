<script lang="ts">
  import { onMount } from "svelte";
  import type { TransitionConfig } from "svelte/transition";
  import {
    getStatus,
    onStatus,
    toggleRecording,
    loadSettings,
    setModel,
    fitWindow,
    openSettings,
  } from "./ipc";
  import { MODELS } from "./models";
  import type { AppStatus } from "./types";

  // --- Backend-driven state -------------------------------------------------
  let status = $state<AppStatus>("idle");
  let errorMessage = $state<string | null>(null);
  let shortcut = $state("CmdOrCtrl+Shift+X");
  let model = $state("gpt-4o-mini-transcribe");

  // --- Local UI state -------------------------------------------------------
  let expanded = $state(false);
  let hovered = $state(false); // mouse anywhere over the window
  /** Button whose text tooltip is shown (only after a sustained hover). */
  let tooltipBtn = $state<null | "model" | "record" | "expand">(null);
  let menuOpen = $state(false);

  // Show the icon row immediately on hover, but only reveal the *text* tooltip
  // after a sustained hover, and delay collapsing back to the empty pill so a
  // grow/shrink loop at the window edge can't make it flicker.
  const TOOLTIP_DELAY = 2000;
  const LEAVE_DELAY = 280;
  /** Length of the shell morph; the window only shrinks once it's done. */
  const MORPH_MS = 520;
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;
  let tipTimer: ReturnType<typeof setTimeout> | undefined;
  let shrinkTimer: ReturnType<typeof setTimeout> | undefined;

  type Size = { w: number; h: number };

  // Window sizes (logical px). Every non-idle compact mode shares one width so
  // switching between them never moves the window sideways. `IDLE` must match
  // `PILL_SIZE` in `src-tauri/src/window.rs`.
  const IDLE: Size = { w: 72, h: 40 };
  const COMPACT_W = 240;
  const PANEL: Size = { w: 500, h: 158 };
  const MENU_H = 216;

  /** The morphing shell's shape for each visual state (CSS px). */
  type Visual = "idle" | "icons" | "rec" | "proc" | "panel";
  const SHELL: Record<Visual, { w: number; h: number; r: number; top: number }> = {
    idle: { w: 40, h: 18, r: 9, top: 4 },
    icons: { w: 102, h: 40, r: 20, top: 4 },
    rec: { w: 88, h: 40, r: 20, top: 4 },
    proc: { w: 56, h: 40, r: 20, top: 4 },
    panel: { w: PANEL.w, h: PANEL.h, r: 16, top: 0 },
  };

  const STATUS_TEXT: Record<AppStatus, string> = {
    idle: "Listo para dictar",
    recording: "Grabando…",
    processing: "Transcribiendo…",
    error: "Algo salió mal",
  };

  const busy = $derived(status === "recording" || status === "processing");
  const showIcons = $derived(!busy && (hovered || menuOpen));
  const modelLabel = $derived(MODELS.find((m) => m.id === model)?.label ?? model);

  const visual = $derived.by<Visual>(() => {
    if (expanded) return "panel";
    if (status === "recording") return "rec";
    if (status === "processing") return "proc";
    return showIcons ? "icons" : "idle";
  });
  const shell = $derived(SHELL[visual]);

  // Text tooltip shown only after a sustained hover on a specific icon.
  const tooltip = $derived.by(() => {
    if (busy || menuOpen || !tooltipBtn) return null;
    if (tooltipBtn === "record")
      return status === "error"
        ? { text: errorMessage ?? "Error", keys: [] as string[] }
        : { text: "Start recording", keys: keyParts(shortcut) };
    if (tooltipBtn === "model") return { text: "Modelo", keys: [] };
    return { text: "Expandir", keys: [] };
  });

  /** Window size that fits everything currently shown. */
  const target = $derived.by<Size>(() => {
    if (expanded) return { w: PANEL.w, h: menuOpen ? PANEL.h + 8 + MENU_H + 8 : PANEL.h };
    if (busy) return { w: COMPACT_W, h: 54 };
    if (menuOpen) return { w: COMPACT_W, h: 50 + MENU_H + 8 };
    if (tooltipBtn) return { w: COMPACT_W, h: 112 };
    if (hovered) return { w: COMPACT_W, h: 54 };
    return IDLE;
  });

  // Grow the window *before* the shell morphs (so nothing gets clipped) and
  // shrink it only *after* the morph settles (so the shape can animate down
  // instead of being cut off). Mixed changes grow to the union first.
  let applied: Size = { ...IDLE };
  function applyWindow(s: Size) {
    applied = s;
    fitWindow(s.w, s.h);
  }
  $effect(() => {
    const t = target;
    clearTimeout(shrinkTimer);
    const grown = { w: Math.max(t.w, applied.w), h: Math.max(t.h, applied.h) };
    if (grown.w !== applied.w || grown.h !== applied.h) applyWindow(grown);
    if (grown.w !== t.w || grown.h !== t.h) shrinkTimer = setTimeout(() => applyWindow(t), MORPH_MS);
  });

  /** Pretty-print the configured shortcut into key chips. */
  function keyParts(s: string): string[] {
    return s
      .split("+")
      .map((k) => (k === "CmdOrCtrl" || k === "CommandOrControl" ? "Ctrl" : k));
  }

  onMount(() => {
    getStatus()
      .then((s) => (status = s))
      .catch((e) => console.error("getStatus failed", e));
    loadSettings()
      .then((v) => {
        shortcut = v.shortcut;
        model = v.model;
      })
      .catch((e) => console.error("loadSettings failed", e));

    const unlisten = onStatus((e) => {
      status = e.status;
      errorMessage = e.status === "error" ? e.message ?? "Error desconocido" : null;
    });

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (menuOpen) menuOpen = false;
        else if (expanded) collapse();
      }
    };
    window.addEventListener("keydown", onKey);

    return () => {
      unlisten.then((fn) => fn());
      window.removeEventListener("keydown", onKey);
      clearTimeout(leaveTimer);
      clearTimeout(tipTimer);
      clearTimeout(shrinkTimer);
    };
  });

  /** Also bound to mousemove: after collapsing the panel the cursor may already
   *  be inside the window, where no new mouseenter would ever fire. */
  function enterPill() {
    if (expanded) return;
    clearTimeout(leaveTimer);
    hovered = true;
  }

  /** Delay the collapse so the window's grow/shrink can't oscillate at its edge. */
  function leavePill() {
    clearTimeout(tipTimer);
    tooltipBtn = null;
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => {
      hovered = false;
      if (!expanded) menuOpen = false;
    }, LEAVE_DELAY);
  }

  function enterBtn(which: "model" | "record" | "expand") {
    clearTimeout(tipTimer);
    tipTimer = setTimeout(() => (tooltipBtn = which), TOOLTIP_DELAY);
  }

  function leaveBtn() {
    clearTimeout(tipTimer);
    tooltipBtn = null;
  }

  function record() {
    menuOpen = false;
    toggleRecording();
  }

  function chooseModel(id: string) {
    model = id;
    menuOpen = false;
    setModel(id);
  }

  function expand() {
    clearTimeout(leaveTimer);
    clearTimeout(tipTimer);
    tooltipBtn = null;
    menuOpen = false;
    hovered = false;
    expanded = true;
  }

  function collapse() {
    menuOpen = false;
    expanded = false; // the shell morphs back, then the window shrinks
  }

  /** Popover entrance: unfold from the pill with a soft blur (Apple-like). */
  function pop(_node: Element, { duration = 260 } = {}): TransitionConfig {
    return {
      duration,
      easing: (t) => 1 - Math.pow(1 - t, 3),
      css: (t, u) =>
        `opacity:${t};transform:translateY(${-6 * u}px) scale(${0.94 + 0.06 * t});filter:blur(${4 * u}px)`,
    };
  }
</script>

{#snippet waveform()}
  <span class="waveform" aria-hidden="true">
    {#each Array(5) as _, i (i)}<span style="--i:{i}"></span>{/each}
  </span>
{/snippet}

{#snippet spinner()}
  <span class="spinner" aria-hidden="true"></span>
{/snippet}

{#snippet modelMenu(placement: "compact" | "panel")}
  <div class="menu {placement}" role="menu" transition:pop>
    {#each [false, true] as local (local)}
      <div class="menu-group">{local ? "Local · faster-whisper" : "OpenAI"}</div>
      {#each MODELS.filter((m) => m.local === local) as m (m.id)}
        <button class="menu-item" class:active={m.id === model} role="menuitemradio" aria-checked={m.id === model} onclick={() => chooseModel(m.id)}>
          <span class="mic" aria-hidden="true">{#if m.local}{@render chipIcon()}{:else}{@render micIcon()}{/if}</span>
          <span class="menu-label">{m.label}</span>
          {#if m.id === model}<span class="check" aria-hidden="true">✓</span>{/if}
        </button>
      {/each}
    {/each}
  </div>
{/snippet}

<!-- Inline icons (kept tiny and dependency-free). -->
{#snippet sparkleIcon()}
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor"><path d="M12 2l1.8 5.6a4 4 0 0 0 2.6 2.6L22 12l-5.6 1.8a4 4 0 0 0-2.6 2.6L12 22l-1.8-5.6a4 4 0 0 0-2.6-2.6L2 12l5.6-1.8a4 4 0 0 0 2.6-2.6L12 2z"/></svg>
{/snippet}
{#snippet recordIcon()}
  <svg viewBox="0 0 24 24" width="16" height="16"><rect x="8.5" y="2" width="7" height="12.5" rx="3.5" fill="currentColor"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3.5M8.5 21.5h7" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
{/snippet}
{#snippet expandIcon()}
  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H3v-6M21 9V3h-6M3 21l7-7M21 3l-7 7"/></svg>
{/snippet}
{#snippet collapseIcon()}
  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 10h6V4M20 14h-6v6M14 10l7-7M10 14l-7 7"/></svg>
{/snippet}
{#snippet gearIcon()}
  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
{/snippet}
{#snippet micIcon()}
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4"/></svg>
{/snippet}
{#snippet chipIcon()}
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="5" width="14" height="14" rx="2"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/></svg>
{/snippet}

<div class="stage" role="group" onmouseenter={enterPill} onmousemove={enterPill} onmouseleave={leavePill}>
  <!--
    One persistent shell for every state. Its size/radius morph on a spring
    while each state's content cross-fades in its own layer, so changes read
    as the same object transforming instead of elements being swapped.
  -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="shell {status} {visual}"
    style="width:{shell.w}px;height:{shell.h}px;top:{shell.top}px;border-radius:{shell.r}px"
    data-tauri-drag-region
    title={visual === "rec" ? "Detener" : undefined}
    aria-label={busy ? STATUS_TEXT[status] : undefined}
    onclick={visual === "rec" ? record : undefined}
  >
    <!-- Hover: model · record · expand -->
    <div class="layer" class:on={visual === "icons"} inert={visual !== "icons"}>
      <button
        class="icon-btn side"
        class:active={menuOpen}
        aria-label="Elegir modelo"
        onmouseenter={() => enterBtn("model")}
        onmouseleave={leaveBtn}
        onclick={() => (menuOpen = !menuOpen)}
      >
        {@render sparkleIcon()}
      </button>
      <button
        class="icon-btn record"
        aria-label="Grabar"
        onmouseenter={() => enterBtn("record")}
        onmouseleave={leaveBtn}
        onclick={(e) => {
          // Don't let the click reach the shell, which becomes "stop" once recording.
          e.stopPropagation();
          record();
        }}
      >
        {@render recordIcon()}
      </button>
      <button
        class="icon-btn side"
        aria-label="Expandir"
        onmouseenter={() => enterBtn("expand")}
        onmouseleave={leaveBtn}
        onclick={expand}
      >
        {@render expandIcon()}
      </button>
    </div>

    <!-- Recording: live dot + waveform (click to stop) -->
    <div class="layer" class:on={visual === "rec"} data-tauri-drag-region>
      <span class="rec-dot" aria-hidden="true"></span>
      {@render waveform()}
    </div>

    <!-- Processing -->
    <div class="layer" class:on={visual === "proc"} data-tauri-drag-region>
      {@render spinner()}
    </div>

    <!-- Expanded panel -->
    <div class="layer panel-layer" class:on={visual === "panel"} inert={visual !== "panel"} data-tauri-drag-region>
      <button class="icon-btn collapse" title="Colapsar (Esc)" aria-label="Colapsar" onclick={collapse}>
        {@render collapseIcon()}
      </button>

      <div class="panel-body" data-tauri-drag-region>
        <div class="pb-layer" class:on={status === "idle"} data-tauri-drag-region>
          <p class="panel-status">{STATUS_TEXT.idle}</p>
        </div>
        <div class="pb-layer" class:on={status === "recording"} data-tauri-drag-region>
          <p class="panel-status">{STATUS_TEXT.recording}</p>
          <div class="wave-big">{@render waveform()}</div>
        </div>
        <div class="pb-layer" class:on={status === "processing"} data-tauri-drag-region>
          <p class="panel-status">{STATUS_TEXT.processing}</p>
          {@render spinner()}
        </div>
        <div class="pb-layer" class:on={status === "error"} data-tauri-drag-region>
          <p class="panel-status err">{errorMessage ?? STATUS_TEXT.error}</p>
        </div>
      </div>

      <div class="panel-bar">
        <button class="model-btn" class:active={menuOpen} onclick={() => (menuOpen = !menuOpen)}>
          <span class="mic" aria-hidden="true">{@render micIcon()}</span>
          <span>{modelLabel}</span>
        </button>

        <div class="hints">
          <button class="hint rec-hint" onclick={record}>
            {status === "recording" ? "Stop" : "Record"}
            {#each keyParts(shortcut) as k (k)}<kbd>{k}</kbd>{/each}
          </button>
          <span class="hint">Close <kbd>Esc</kbd></span>
          <button class="icon-btn" title="Ajustes" aria-label="Ajustes" onclick={openSettings}>
            {@render gearIcon()}
          </button>
        </div>
      </div>
    </div>
  </div>

  {#if tooltip && !expanded}
    <div class="tooltip" transition:pop={{ duration: 200 }}>
      <span class="tt-text" class:err={status === "error" && tooltipBtn === "record"}>{tooltip.text}</span>
      {#if tooltip.keys.length}
        <span class="tt-keys">{#each tooltip.keys as k (k)}<kbd>{k}</kbd>{/each}</span>
      {/if}
    </div>
  {/if}

  {#if menuOpen}{@render modelMenu(expanded ? "panel" : "compact")}{/if}
</div>

<style>
  :global(#app) {
    /* Pill UI uses fixed px sizing, independent of the settings window scale. */
    font-size: 13px;
  }

  .stage {
    /* Critically-damped-ish spring (~4% overshoot), with a bezier fallback. */
    --spring: cubic-bezier(0.32, 0.72, 0, 1);
    --morph: 520ms;
    position: relative;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
  }
  @supports (transition-timing-function: linear(0, 1)) {
    .stage {
      --spring: linear(0, 0.0402, 0.1381, 0.2657, 0.4029, 0.536, 0.6565, 0.7599, 0.8445, 0.9106, 0.9599, 0.9946, 1.0174, 1.0308, 1.0371, 1.0384, 1.0363, 1.0323, 1.0272, 1.022, 1.0169, 1.0124, 1.0086, 1.0055, 1.003, 1.0013, 1, 0.9992, 0.9988, 0.9986, 0.9985, 0.9986, 0.9988, 0.999, 0.9992, 0.9994, 1);
    }
  }

  /* ---------- Morphing shell ---------- */
  .shell {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    overflow: hidden;
    background: rgba(30, 30, 34, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.1);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    transition:
      width var(--morph) var(--spring),
      height var(--morph) var(--spring),
      top var(--morph) var(--spring),
      border-radius var(--morph) var(--spring),
      background-color 0.35s ease,
      border-color 0.35s ease,
      box-shadow 0.35s ease;
    will-change: width, height;
  }
  .shell.rec {
    cursor: pointer;
  }
  .shell.recording {
    background: rgba(40, 22, 24, 0.95);
    border-color: rgba(248, 113, 113, 0.45);
  }
  .shell.error {
    border-color: rgba(248, 113, 113, 0.55);
  }
  .shell.panel {
    background: rgba(26, 26, 30, 0.96);
    border-color: rgba(255, 255, 255, 0.08);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.4);
  }
  .shell.panel.recording {
    border-color: rgba(248, 113, 113, 0.4);
  }

  /*
   * Content layers: stacked and centered on the shell. Leaving content fades
   * out fast; entering content follows the shape with a small delay, scaling
   * up from a soft blur — the "same object transforming" feel.
   */
  .layer {
    position: absolute;
    left: 50%;
    top: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    opacity: 0;
    transform: translate(-50%, -50%) scale(0.6);
    filter: blur(6px);
    pointer-events: none;
    transition:
      opacity 0.14s ease,
      transform 0.3s ease,
      filter 0.2s ease;
  }
  .layer.on {
    opacity: 1;
    transform: translate(-50%, -50%) scale(1);
    filter: blur(0);
    pointer-events: auto;
    transition:
      opacity 0.26s ease 0.07s,
      transform var(--morph) var(--spring) 0.03s,
      filter 0.3s ease 0.07s;
  }

  .rec-dot {
    width: 8px;
    height: 8px;
    margin-right: 10px;
    border-radius: 50%;
    background: #f87171;
    box-shadow: 0 0 0 0 rgba(248, 113, 113, 0.6);
    animation: pulse 1.4s ease-out infinite;
  }
  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(248, 113, 113, 0.55);
    }
    100% {
      box-shadow: 0 0 0 7px rgba(248, 113, 113, 0);
    }
  }

  /* ---------- Buttons ---------- */
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: #c9c9d1;
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s,
      transform 0.25s var(--spring);
  }
  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }
  .icon-btn:active {
    transform: scale(0.9);
  }
  .icon-btn.side.active {
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
  }
  .icon-btn.record {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }
  .icon-btn.record:hover {
    background: rgba(255, 255, 255, 0.22);
  }

  /* ---------- Tooltip / menu (below the compact pill) ---------- */
  .tooltip,
  .menu.compact {
    position: absolute;
    top: 50px;
    left: 50%;
    translate: -50% 0;
    transform-origin: top center;
  }
  .tooltip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    background: rgba(28, 28, 32, 0.95);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.3);
    white-space: nowrap;
    color: #d8d8df;
  }
  .tt-text {
    font-weight: 500;
  }
  .tt-text.err {
    color: #f87171;
  }
  .tt-keys {
    display: inline-flex;
    gap: 3px;
  }

  .menu {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    border-radius: 12px;
    background: rgba(28, 28, 32, 0.96);
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    min-width: 214px;
  }
  /* Expanded: drop below the panel, aligned with the model button. */
  .menu.panel {
    position: absolute;
    top: 166px;
    left: 12px;
    transform-origin: top left;
  }
  .menu-group {
    padding: 6px 9px 3px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #75757e;
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 9px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: #d8d8df;
    cursor: pointer;
    text-align: left;
    font-size: 12px;
    transition: background 0.12s;
  }
  .menu-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }
  .menu-item.active {
    color: #fff;
  }
  .menu-label {
    flex: 1;
  }
  .check {
    color: #8ab4ff;
  }
  .mic {
    display: inline-flex;
    color: #9a9aa2;
  }

  /* ---------- Waveform / spinner ---------- */
  .waveform {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 18px;
  }
  .waveform span {
    width: 3px;
    height: 100%;
    border-radius: 2px;
    background: #f1f1f4;
    transform-origin: center;
    animation: bar 0.9s ease-in-out infinite;
    animation-delay: calc(var(--i) * 0.12s);
  }
  @keyframes bar {
    0%,
    100% {
      transform: scaleY(0.3);
    }
    50% {
      transform: scaleY(1);
    }
  }

  .spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: #f1f1f4;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ---------- Expanded panel ---------- */
  .panel-layer {
    /* Fixed panel-sized content, pinned to the shell's top so it is revealed
       by the growing shape rather than squeezed by it. */
    top: 0;
    width: 500px;
    height: 158px;
    flex-direction: column;
    align-items: stretch;
    justify-content: flex-start;
    gap: 0;
    transform: translate(-50%, 0) scale(0.9);
    transform-origin: top center;
  }
  .panel-layer.on {
    transform: translate(-50%, 0) scale(1);
  }
  .collapse {
    position: absolute;
    top: 10px;
    right: 10px;
    z-index: 1;
  }
  .panel-body {
    position: relative;
    flex: 1;
  }
  .pb-layer {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    opacity: 0;
    transform: scale(0.94);
    filter: blur(4px);
    transition:
      opacity 0.14s ease,
      transform 0.3s ease,
      filter 0.2s ease;
  }
  .pb-layer.on {
    opacity: 1;
    transform: none;
    filter: none;
    transition:
      opacity 0.26s ease 0.06s,
      transform var(--morph) var(--spring),
      filter 0.3s ease 0.06s;
  }
  .panel-status {
    margin: 0;
    font-size: 15px;
    color: #cfcfd6;
  }
  .panel-status.err {
    color: #f87171;
    max-width: 80%;
    text-align: center;
  }
  .wave-big {
    transform: scale(1.6);
  }

  .panel-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    background: rgba(0, 0, 0, 0.25);
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }
  .model-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    border: none;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    color: #d8d8df;
    cursor: pointer;
    font-size: 12px;
    transition: background 0.15s;
  }
  .model-btn:hover,
  .model-btn.active {
    background: rgba(255, 255, 255, 0.12);
  }
  .hints {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: #9a9aa2;
  }
  .rec-hint {
    padding: 4px 6px;
    margin: -4px -6px;
    border: none;
    border-radius: 8px;
    background: transparent;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .rec-hint:hover {
    background: rgba(255, 255, 255, 0.08);
    color: #d8d8df;
  }

  kbd {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.12);
    font-family: inherit;
    font-size: 11px;
    color: #e6e6e6;
    line-height: 1.4;
  }

  @media (prefers-reduced-motion: reduce) {
    .stage {
      --morph: 1ms;
    }
  }
</style>
