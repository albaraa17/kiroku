<script lang="ts">
  import { appStore } from '$lib/stores/app.svelte';
  import { CODEPAGE_OPTIONS } from '$lib/constants';

  let threadSearch = $state('');
  let hookcodeInput = $state('');
  let showRawDiff = $state(true);
  let showConsoleLogs = $state(false);

  let attachedProc = $derived(appStore.snapshot.attached_processes[0] ?? null);

  let filteredThreads = $derived.by(() => {
    const q = threadSearch.trim().toLowerCase();
    if (!q) return appStore.snapshot.threads;
    return appStore.snapshot.threads.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.hookcode.toLowerCase().includes(q) ||
        t.latest_cleaned.toLowerCase().includes(q) ||
        t.latest_raw.toLowerCase().includes(q),
    );
  });

  let selectedThread = $derived(
    appStore.snapshot.threads.find(
      (t) => t.key === appStore.snapshot.selected_thread_key,
    ) ?? null,
  );

  async function handleFlushDelayChange(e: Event) {
    const val = parseInt((e.currentTarget as HTMLInputElement).value, 10);
    if (!appStore.config || Number.isNaN(val)) return;
    const clamped = Math.max(0, Math.min(5000, val));
    await appStore.saveConfig(
      { ...appStore.config, flush_delay: clamped },
      true,
    );
  }

  async function handleCodepageChange(e: Event) {
    const val = parseInt((e.currentTarget as HTMLSelectElement).value, 10);
    if (!appStore.config || Number.isNaN(val)) return;
    await appStore.saveConfig(
      { ...appStore.config, default_codepage: val },
      true,
    );
  }

  async function toggleClipboard() {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      to_clipboard: !appStore.config.to_clipboard,
    });
  }

  async function toggleHideGarbage() {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      hide_garbage_threads: !appStore.config.hide_garbage_threads,
    });
  }

  async function toggleBacklogGuard() {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      backlog_filter_enabled: !appStore.config.backlog_filter_enabled,
    });
  }

  async function toggleSkipGuard() {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      skip_filter_enabled: !appStore.config.skip_filter_enabled,
    });
  }

  async function submitHookCode(e: SubmitEvent) {
    e.preventDefault();
    if (!hookcodeInput.trim()) return;
    await appStore.insertHookCode(hookcodeInput);
    hookcodeInput = '';
  }
</script>

<div class="console-root">
  <!-- Top Status & Quick Hook Engine Bar -->
  <header class="status-bar">
    <div class="attached-card">
      {#if attachedProc}
        <div class="proc-icon">
          {#if attachedProc.icon_data_url}
            <img src={attachedProc.icon_data_url} alt={attachedProc.name} />
          {:else}
            <i class="fa-solid fa-gamepad"></i>
          {/if}
        </div>
        <div class="proc-meta">
          <div class="proc-title-row">
            <span class="proc-name">{attachedProc.name}</span>
            <span class="arch-badge" class:knife={attachedProc.is_knife}>
              {attachedProc.is_knife ? 'WS' : attachedProc.is_64bit ? 'x64' : 'x86'}
            </span>
            <span class="pid-badge">PID {attachedProc.pid}</span>
          </div>
          <div class="proc-path" title={attachedProc.exe_path}>
            {attachedProc.exe_path}
          </div>
        </div>
        <div class="proc-actions">
          <button
            class="action-btn"
            onclick={() => {
              appStore.activeTab = 'processes';
              appStore.refreshProcesses();
            }}
            title="Switch Process"
          >
            <i class="fa-solid fa-retweet"></i> Switch
          </button>
          <button
            class="danger-btn"
            onclick={() => appStore.detachProcess(attachedProc.pid)}
            title="Detach Process"
          >
            <i class="fa-solid fa-link-slash"></i> Detach
          </button>
        </div>
      {:else}
        <div class="proc-icon idle">
          <i class="fa-solid fa-plug"></i>
        </div>
        <div class="proc-meta">
          <div class="proc-title-row">
            <span class="proc-name">No Process Attached</span>
            {#if appStore.config?.auto_attach}
              <span class="auto-badge">
                <i class="fa-solid fa-radar"></i> Auto-Attach Active
              </span>
            {/if}
          </div>
          <div class="proc-path">
            Select a running visual novel or wait for a saved game / Knife WS to start
          </div>
        </div>
        <button
          class="primary-btn"
          onclick={() => {
            appStore.activeTab = 'processes';
            appStore.refreshProcesses();
          }}
        >
          <i class="fa-solid fa-gamepad"></i> Select Process
        </button>
      {/if}
    </div>

    <!-- Quick Hook Engine Controls (Flush Delay, Codepage, Output) -->
    {#if appStore.config}
      <div class="quick-controls">
        <div class="control-pill" title="Flush Delay in milliseconds (Luna_Settings)">
          <span class="ctrl-label"><i class="fa-solid fa-stopwatch"></i> Flush Delay</span>
          <input
            type="number"
            min="0"
            max="5000"
            step="10"
            value={appStore.config.flush_delay}
            onchange={handleFlushDelayChange}
            class="mini-number"
          />
          <span class="unit">ms</span>
        </div>

        <div class="control-pill" title="Default Text Encoding Codepage">
          <span class="ctrl-label"><i class="fa-solid fa-language"></i> Codepage</span>
          <select
            value={appStore.config.default_codepage}
            onchange={handleCodepageChange}
            class="mini-select"
          >
            {#each CODEPAGE_OPTIONS as opt (opt.value)}
              <option value={opt.value}>{opt.value}</option>
            {/each}
          </select>
        </div>

        <button
          class="toggle-pill"
          class:active={appStore.config.to_clipboard}
          onclick={toggleClipboard}
          title="Automatically copy cleaned sentences to Windows Clipboard"
        >
          <i class="fa-regular fa-clipboard"></i>
          Clipboard {appStore.config.to_clipboard ? 'ON' : 'OFF'}
        </button>

        <button
          class="toggle-pill"
          class:active={appStore.config.backlog_filter_enabled}
          onclick={toggleBacklogGuard}
          title="Smart Backlog Protection (suppresses scroll wheel and backlog screen floods)"
        >
          <i class="fa-solid fa-shield-halved"></i>
          Backlog Guard {appStore.config.backlog_filter_enabled ? 'ON' : 'OFF'}
        </button>

        <button
          class="toggle-pill"
          class:active={appStore.config.skip_filter_enabled}
          onclick={toggleSkipGuard}
          title="Fast-Skip Flood Protection (suppresses rapid skip bursts & multi-turn dumps, sending only the landing line after {appStore.config.skip_settle_ms}ms)"
        >
          <i class="fa-solid fa-forward-fast"></i>
          Skip Guard {appStore.config.skip_filter_enabled ? 'ON' : 'OFF'}
        </button>

        <button
          class="action-btn"
          onclick={() => appStore.insertPcHooks(0)}
          disabled={!attachedProc || attachedProc.is_knife}
          title="Insert GDI / Standard Windows Text API Hooks"
        >
          <i class="fa-solid fa-microchip"></i> +GDI Hooks
        </button>
      </div>
    {/if}
  </header>

  <!-- Split Workspace: Left = Ranked Hook Threads, Right = Selected Thread Live Stream -->
  <div class="workspace-grid">
    <!-- Left Column: Ranked Hook Threads Feed -->
    <section class="threads-pane">
      <div class="pane-header">
        <div class="pane-title">
          <i class="fa-solid fa-list-check"></i>
          <span>Text Threads ({filteredThreads.length})</span>
        </div>
        <button
          class="filter-chip"
          class:active={appStore.config?.hide_garbage_threads}
          onclick={toggleHideGarbage}
          title="Hide low-score memory noise threads"
        >
          <i class="fa-solid fa-filter"></i> Hide Noise
        </button>
      </div>

      <!-- Search + Insert Hookcode Bar -->
      <div class="thread-toolbar">
        <div class="search-box">
          <i class="fa-solid fa-magnifying-glass"></i>
          <input
            type="text"
            placeholder="Filter hooks or text..."
            bind:value={threadSearch}
          />
        </div>
        <form class="hookcode-form" onsubmit={submitHookCode}>
          <input
            type="text"
            placeholder="Insert /H code..."
            bind:value={hookcodeInput}
          />
          <button type="submit" title="Insert User Hook Code">
            <i class="fa-solid fa-plus"></i>
          </button>
        </form>
      </div>

      <!-- Scrollable Thread Cards -->
      <div class="thread-list">
        {#if filteredThreads.length === 0}
          <div class="empty-state">
            <i class="fa-solid fa-wave-square"></i>
            <p>No active text threads yet</p>
            <span>
              Attach to a game and advance dialogue in-game (or stream via Knife WS) to see hooks appear here automatically ranked by quality.
            </span>
          </div>
        {:else}
          {#each filteredThreads as thread (thread.key)}
            {@const isSelected = appStore.snapshot.selected_thread_key === thread.key}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="thread-card"
              class:selected={isSelected}
              class:saved={thread.is_saved_match}
              onclick={() => appStore.selectThread(thread.key, true)}
            >
              <div class="thread-card-top">
                <div class="thread-badges">
                  {#if thread.is_saved_match}
                    <span class="saved-badge" title="Saved & auto-selected for this game">
                      <i class="fa-solid fa-star"></i> Saved
                    </span>
                  {/if}
                  {#if thread.is_knife}
                    <span class="knife-badge">
                      <i class="fa-solid fa-bolt"></i> Knife WS
                    </span>
                  {/if}
                  <span class="hook-name">{thread.name || 'Hook'}</span>
                  <code class="hook-code">{thread.hookcode}</code>
                </div>

                <div class="thread-card-right">
                  <span
                    class="score-pill"
                    class:high={thread.score >= 70}
                    class:low={thread.score < 35}
                    title="Japanese / Text Quality Score"
                  >
                    {thread.score}%
                  </span>
                  <button
                    class="icon-btn"
                    onclick={(e) => {
                      e.stopPropagation();
                      appStore.copyText(thread.hookcode, `Copied hookcode: ${thread.hookcode}`);
                    }}
                    title="Copy Hook Code"
                  >
                    <i class="fa-regular fa-copy"></i>
                  </button>
                  {#if !thread.is_knife}
                    <button
                      class="icon-btn danger"
                      onclick={(e) => {
                        e.stopPropagation();
                        appStore.removeHook(thread.tp.process_id, thread.tp.addr);
                      }}
                      title="Remove Hook"
                    >
                      <i class="fa-solid fa-xmark"></i>
                    </button>
                  {/if}
                </div>
              </div>

              <div class="thread-preview">
                {thread.latest_cleaned || thread.latest_raw || 'Waiting for text output...'}
              </div>

              <div class="thread-meta-row">
                <span class="ctx-info">
                  ctx: {thread.tp.ctx.toString(16).toUpperCase()}:{thread.tp.ctx2.toString(16).toUpperCase()}
                </span>
                <span class="count-info">
                  {thread.total_sentences} line{thread.total_sentences === 1 ? '' : 's'} • {thread.total_chars} chars
                </span>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </section>

    <!-- Right Column: Live Output Stream & Diff -->
    <section class="output-pane">
      <div class="pane-header">
        <div class="pane-title">
          <i class="fa-solid fa-terminal"></i>
          {#if selectedThread}
            <span>
              Active Stream: <strong>{selectedThread.name}</strong>
              <code class="header-code">{selectedThread.hookcode}</code>
            </span>
          {:else}
            <span>Live Sentence Broadcast Stream</span>
          {/if}
        </div>

        <div class="stream-header-actions">
          <span class="session-counter">
            <i class="fa-solid fa-font"></i>
            {appStore.snapshot.total_session_chars.toLocaleString()} chars ({appStore.snapshot.total_session_sentences} lines)
          </span>
          <button
            class="filter-chip"
            class:active={showRawDiff}
            onclick={() => (showRawDiff = !showRawDiff)}
            title="Show raw text when cleanup rules modified the sentence"
          >
            <i class="fa-solid fa-code-compare"></i> Show Raw Diff
          </button>
          <button
            class="filter-chip"
            class:active={showConsoleLogs}
            onclick={() => (showConsoleLogs = !showConsoleLogs)}
            title="Toggle LunaHost Engine Console Logs"
          >
            <i class="fa-solid fa-rectangle-list"></i> Logs
          </button>
        </div>
      </div>

      <!-- Sentence History Feed -->
      <div class="sentence-feed">
        {#if !selectedThread}
          <div class="empty-state">
            <i class="fa-solid fa-hand-pointer"></i>
            <p>Select a Hook Thread on the left</p>
            <span>
              Clicking any hook thread immediately routes its cleaned sentences to exSTATic (`ws://127.0.0.1:{appStore.snapshot.ws_port}`) and remembers it for the next time you launch the game.
            </span>
          </div>
        {:else if selectedThread.history.length === 0}
          <div class="empty-state">
            <i class="fa-solid fa-comment-dots"></i>
            <p>Listening on {selectedThread.name}...</p>
            <span>Advance the text in-game to capture and broadcast sentences.</span>
          </div>
        {:else}
          {#each [...selectedThread.history].reverse() as item (item.id)}
            <div class="sentence-card" class:backlog-dimmed={item.is_backlog || item.is_skip}>
              <div class="sentence-header">
                <span class="sentence-time">{item.timestamp}</span>
                <span class="sentence-chars">{item.char_count} chars</span>
                {#if item.is_skip}
                  <span class="skip-badge" title="Suppressed fast-skip flood line (blocked from exSTATic / clipboard; only the final landing line is sent)">
                    <i class="fa-solid fa-forward-fast"></i> [SKIP]
                  </span>
                {:else if item.is_backlog}
                  <span class="backlog-badge" title="Suppressed backlog flood line (blocked from exSTATic / clipboard)">
                    <i class="fa-solid fa-clock-rotate-left"></i> [BACKLOG]
                  </span>
                {/if}
                {#if item.raw.trim() !== item.cleaned.trim()}
                  <span class="cleaned-badge">
                    <i class="fa-solid fa-wand-magic-sparkles"></i> Cleaned
                  </span>
                {/if}
                <div class="sentence-actions">
                  <button
                    class="icon-btn"
                    onclick={() => appStore.copyText(item.cleaned)}
                    title="Copy sentence"
                  >
                    <i class="fa-regular fa-copy"></i>
                  </button>
                  <button
                    class="icon-btn"
                    onclick={() => appStore.broadcastTestSentence(item.cleaned)}
                    title="Re-broadcast sentence to exSTATic WebSocket"
                  >
                    <i class="fa-solid fa-paper-plane"></i>
                  </button>
                </div>
              </div>

              <div class="sentence-text">{item.cleaned}</div>

              {#if showRawDiff && item.raw.trim() !== item.cleaned.trim()}
                <div class="raw-diff">
                  <span class="raw-tag">RAW</span>
                  <span>{item.raw}</span>
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>

      {#if showConsoleLogs}
        <div class="console-drawer">
          <div class="drawer-title">LunaHost Engine Logs</div>
          <div class="drawer-body">
            {#each appStore.snapshot.console_logs as log, idx (idx)}
              <div class="log-line">{log}</div>
            {/each}
          </div>
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .console-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    padding: 1rem 1.25rem;
    gap: 1rem;
    overflow: hidden;
  }

  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    background: var(--accent);
    border-radius: var(--big-radius);
    padding: 0.75rem 1rem;
    border: 1px solid rgba(255, 255, 255, 0.06);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    flex-wrap: wrap;
  }

  .attached-card {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    min-width: 0;
    flex: 1;
  }

  .proc-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: rgba(185, 154, 250, 0.14);
    border: 1px solid rgba(185, 154, 250, 0.25);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    color: var(--primary);
    flex-shrink: 0;

    &.idle {
      background: rgba(255, 255, 255, 0.04);
      border-color: rgba(255, 255, 255, 0.08);
      color: var(--secondary-text);
    }

    & img {
      width: 30px;
      height: 30px;
      object-fit: contain;
    }
  }

  .proc-meta {
    min-width: 0;
    flex: 1;
  }

  .proc-title-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .proc-name {
    font-weight: 700;
    font-size: 16px;
    color: var(--main-text);
  }

  .arch-badge {
    font-size: 11px;
    font-weight: 700;
    padding: 1px 7px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--primary) 22%, transparent);
    color: var(--primary);
    &.knife {
      background: rgba(122, 162, 247, 0.2);
      color: #7aa2f7;
    }
  }

  .pid-badge {
    font-size: 11px;
    color: var(--secondary-text);
    font-family: monospace;
  }

  .auto-badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    background: rgba(158, 206, 106, 0.15);
    color: #9ece6a;
    font-weight: 600;
  }

  .proc-path {
    font-size: 12px;
    color: var(--secondary-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: 2px;
  }

  .proc-actions {
    display: flex;
    gap: 0.5rem;
  }

  .quick-controls {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .control-pill {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    background: var(--main-background);
    padding: 0.35rem 0.65rem;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    font-size: 12px;
  }

  .ctrl-label {
    color: var(--secondary-text);
    font-weight: 500;
  }

  .mini-number {
    width: 62px;
    background: var(--accent);
    color: var(--main-text);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    padding: 2px 6px;
    font-size: 12px;
    text-align: right;
  }

  .mini-select {
    background: var(--accent);
    color: var(--main-text);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    padding: 2px 6px;
    font-size: 12px;
  }

  .unit {
    color: var(--secondary-text);
    font-size: 11px;
  }

  .toggle-pill {
    background: var(--main-background);
    color: var(--secondary-text);
    border: 1px solid rgba(255, 255, 255, 0.08);
    padding: 0.42rem 0.75rem;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;
    &.active {
      background: color-mix(in srgb, var(--primary) 22%, var(--main-background));
      color: var(--primary);
      border-color: var(--primary);
    }
  }

  .primary-btn {
    background: var(--primary);
    color: #111;
    border: none;
    border-radius: var(--small-radius);
    padding: 0.55rem 1rem;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
    transition: filter 0.15s ease;
    &:hover {
      filter: brightness(1.1);
    }
  }

  .action-btn {
    background: var(--main-background);
    color: var(--main-text);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--small-radius);
    padding: 0.45rem 0.75rem;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    &:hover:not(:disabled) {
      border-color: var(--primary);
      color: var(--primary);
    }
    &:disabled {
      opacity: 0.4;
      cursor: not-allowed;
    }
  }

  .danger-btn {
    background: rgba(247, 118, 142, 0.15);
    color: #f7768e;
    border: 1px solid rgba(247, 118, 142, 0.3);
    border-radius: var(--small-radius);
    padding: 0.45rem 0.75rem;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    &:hover {
      background: #f7768e;
      color: #111;
    }
  }

  .workspace-grid {
    display: grid;
    grid-template-columns: 430px 1fr;
    gap: 1rem;
    flex: 1;
    min-height: 0;
  }

  .threads-pane,
  .output-pane {
    background: var(--accent);
    border-radius: var(--big-radius);
    border: 1px solid rgba(255, 255, 255, 0.06);
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .pane-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .pane-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 14px;
    font-weight: 600;
    color: var(--main-text);
    & i {
      color: var(--primary);
    }
  }

  .header-code {
    font-size: 11px;
    background: var(--main-background);
    padding: 2px 6px;
    border-radius: 4px;
    color: var(--secondary-text);
    margin-left: 0.4rem;
  }

  .filter-chip {
    background: var(--main-background);
    color: var(--secondary-text);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    padding: 0.3rem 0.6rem;
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    &.active {
      color: var(--primary);
      border-color: var(--primary);
      background: color-mix(in srgb, var(--primary) 15%, var(--main-background));
    }
  }

  .thread-toolbar {
    display: flex;
    gap: 0.5rem;
    padding: 0.6rem 0.85rem;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }

  .search-box {
    flex: 1;
    position: relative;
    display: flex;
    align-items: center;
    & i {
      position: absolute;
      left: 0.65rem;
      font-size: 12px;
      color: var(--secondary-text);
    }
    & input {
      width: 100%;
      background: var(--main-background);
      border: 1px solid rgba(255, 255, 255, 0.08);
      border-radius: var(--small-radius);
      padding: 0.4rem 0.6rem 0.4rem 1.9rem;
      color: var(--main-text);
      font-size: 12px;
      &:focus {
        outline: 1px solid var(--primary);
      }
    }
  }

  .hookcode-form {
    display: flex;
    gap: 0.25rem;
    width: 155px;
    & input {
      width: 100%;
      background: var(--main-background);
      border: 1px solid rgba(255, 255, 255, 0.08);
      border-radius: var(--small-radius);
      padding: 0.4rem 0.5rem;
      color: var(--main-text);
      font-size: 12px;
      font-family: monospace;
      &:focus {
        outline: 1px solid var(--primary);
      }
    }
    & button {
      background: var(--primary);
      color: #111;
      border: none;
      border-radius: var(--small-radius);
      padding: 0 0.6rem;
      cursor: pointer;
      font-weight: 700;
    }
  }

  .thread-list,
  .sentence-feed {
    flex: 1;
    overflow-y: auto;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .thread-card {
    background: var(--main-background);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 0.7rem 0.85rem;
    cursor: pointer;
    transition: all 0.15s ease;

    &:hover {
      border-color: rgba(255, 255, 255, 0.18);
      transform: translateY(-1px);
    }

    &.selected {
      border-color: var(--primary);
      background: color-mix(in srgb, var(--primary) 10%, var(--main-background));
      box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3);
    }
  }

  .thread-card-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.4rem;
    margin-bottom: 0.45rem;
  }

  .thread-badges {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
    flex-wrap: wrap;
  }

  .saved-badge {
    background: rgba(224, 175, 104, 0.2);
    color: #e0af68;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
  }

  .knife-badge {
    background: rgba(122, 162, 247, 0.2);
    color: #7aa2f7;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
  }

  .hook-name {
    font-weight: 700;
    font-size: 13px;
    color: var(--primary);
  }

  .hook-code {
    font-size: 11px;
    color: var(--secondary-text);
    background: rgba(255, 255, 255, 0.04);
    padding: 1px 5px;
    border-radius: 4px;
    max-width: 170px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .thread-card-right {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }

  .score-pill {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--secondary-text);
    &.high {
      background: rgba(158, 206, 106, 0.16);
      color: #9ece6a;
    }
    &.low {
      background: rgba(247, 118, 142, 0.15);
      color: #f7768e;
    }
  }

  .icon-btn {
    background: transparent;
    border: none;
    color: var(--secondary-text);
    cursor: pointer;
    padding: 3px 5px;
    border-radius: 4px;
    font-size: 12px;
    &:hover {
      background: rgba(255, 255, 255, 0.08);
      color: var(--main-text);
    }
    &.danger:hover {
      color: #f7768e;
    }
  }

  .thread-preview {
    font-size: 13.5px;
    color: var(--main-text);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
    user-select: text;
  }

  .thread-meta-row {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--secondary-text);
    margin-top: 0.45rem;
    font-family: monospace;
  }

  .stream-header-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .session-counter {
    font-size: 12px;
    color: var(--secondary);
    font-weight: 600;
    margin-right: 0.35rem;
  }

  .sentence-card {
    background: var(--main-background);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 0.8rem 1rem;
  }

  .sentence-header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-bottom: 0.4rem;
    font-size: 11px;
    color: var(--secondary-text);
  }

  .sentence-time {
    font-family: monospace;
  }

  .cleaned-badge {
    background: color-mix(in srgb, var(--primary) 18%, transparent);
    color: var(--primary);
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 600;
  }

  .backlog-badge {
    background: rgba(247, 118, 142, 0.18);
    color: #f7768e;
    border: 1px solid rgba(247, 118, 142, 0.35);
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 700;
    font-size: 0.72rem;
    letter-spacing: 0.03em;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }

  .skip-badge {
    background: rgba(224, 175, 104, 0.18);
    color: #e0af68;
    border: 1px solid rgba(224, 175, 104, 0.38);
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 700;
    font-size: 0.72rem;
    letter-spacing: 0.03em;
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
  }

  .sentence-card.backlog-dimmed {
    opacity: 0.52;
    background: rgba(255, 255, 255, 0.02);
    border-style: dashed;
    border-color: rgba(247, 118, 142, 0.3);
  }

  .sentence-card.backlog-dimmed:hover {
    opacity: 0.88;
  }

  .sentence-actions {
    margin-left: auto;
    display: flex;
    gap: 0.25rem;
  }

  .sentence-text {
    font-size: 16px;
    line-height: 1.6;
    color: var(--main-text);
    user-select: text;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .raw-diff {
    margin-top: 0.5rem;
    padding-top: 0.45rem;
    border-top: 1px dashed rgba(255, 255, 255, 0.08);
    font-size: 12px;
    color: var(--secondary-text);
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    user-select: text;
  }

  .raw-tag {
    font-size: 10px;
    font-weight: 700;
    background: rgba(247, 118, 142, 0.16);
    color: #f7768e;
    padding: 1px 5px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .console-drawer {
    height: 135px;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    background: #141418;
    padding: 0.5rem 0.85rem;
    display: flex;
    flex-direction: column;
  }

  .drawer-title {
    font-size: 11px;
    font-weight: 700;
    color: var(--secondary-text);
    margin-bottom: 0.3rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .drawer-body {
    flex: 1;
    overflow-y: auto;
    font-family: monospace;
    font-size: 11.5px;
    color: #9ca3af;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .empty-state {
    margin: auto;
    text-align: center;
    max-width: 320px;
    padding: 2rem 1rem;
    color: var(--secondary-text);
    & i {
      font-size: 32px;
      color: var(--primary);
      opacity: 0.7;
      margin-bottom: 0.75rem;
    }
    & p {
      font-size: 15px;
      font-weight: 600;
      color: var(--main-text);
      margin-bottom: 0.35rem;
    }
    & span {
      font-size: 12.5px;
      line-height: 1.5;
    }
  }
</style>
