<script lang="ts">
  import { onMount } from 'svelte';
  import { appStore } from '$lib/stores/app.svelte';

  let searchQuery = $state('');
  let subTab = $state<'running' | 'saved'>('running');

  onMount(() => {
    appStore.refreshProcesses();
  });

  let filteredProcesses = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return appStore.processes;
    return appStore.processes.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.title.toLowerCase().includes(q) ||
        p.exe_path.toLowerCase().includes(q) ||
        String(p.pid).includes(q),
    );
  });

  let savedEntries = $derived.by(() => {
    const cfg = appStore.config;
    if (!cfg) return [];
    return cfg.auto_attach_exes.map((exe) => ({
      exe_path: exe,
      name: exe.split('\\').pop() || exe,
      saved_hook: cfg.saved_hooks[exe] ?? null,
    }));
  });

  async function removeSavedGame(exePath: string) {
    if (!appStore.config) return;
    const nextExes = appStore.config.auto_attach_exes.filter(
      (e) => e !== exePath,
    );
    const nextHooks = { ...appStore.config.saved_hooks };
    delete nextHooks[exePath];
    await appStore.saveConfig(
      {
        ...appStore.config,
        auto_attach_exes: nextExes,
        saved_hooks: nextHooks,
      },
      true,
    );
  }
</script>

<div class="picker-root">
  <header class="picker-header">
    <div>
      <h2><i class="fa-solid fa-gamepad"></i> Attach Game / Text Source</h2>
      <p>
        Select a running windowed game to inject LunaHook, connect to Knife WebSocket, or manage saved auto-attach profiles.
      </p>
    </div>

    <div class="header-actions">
      <div class="tab-pills">
        <button
          class:active={subTab === 'running'}
          onclick={() => (subTab = 'running')}
        >
          Running Windows ({appStore.processes.length})
        </button>
        <button
          class:active={subTab === 'saved'}
          onclick={() => (subTab = 'saved')}
        >
          Saved Games ({savedEntries.length})
        </button>
      </div>

      <button
        class="refresh-btn"
        onclick={() => appStore.refreshProcesses()}
        disabled={appStore.loadingProcesses}
      >
        <i
          class="fa-solid fa-rotate"
          class:fa-spin={appStore.loadingProcesses}
        ></i>
        Refresh
      </button>
    </div>
  </header>

  {#if subTab === 'running'}
    <div class="search-row">
      <i class="fa-solid fa-magnifying-glass"></i>
      <input
        id="game-search-input"
        type="text"
        placeholder="Search by window title, executable name, or PID..."
        bind:value={searchQuery}
      />
    </div>

    <div class="process-grid">
      {#each filteredProcesses as proc (proc.pid)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="proc-card"
          class:saved={proc.is_saved}
          onclick={() => appStore.attachProcess(proc.pid)}
        >
          <div class="card-icon">
            {#if proc.icon_data_url}
              <img src={proc.icon_data_url} alt={proc.name} />
            {:else}
              <i class="fa-solid fa-window-maximize"></i>
            {/if}
          </div>

          <div class="card-info">
            <div class="card-title-row">
              <span class="win-title" title={proc.title}>{proc.title}</span>
              {#if proc.is_saved}
                <span class="saved-pill"><i class="fa-solid fa-star"></i> Saved</span>
              {/if}
              {#if proc.is_knife}
                <span class="knife-pill"><i class="fa-solid fa-bolt"></i> Knife</span>
              {/if}
            </div>
            <div class="exe-row">
              <span class="exe-name">{proc.name}</span>
              <span class="bit-badge">{proc.is_64bit ? 'x64' : 'x86'}</span>
              <span class="pid-text">PID {proc.pid}</span>
            </div>
            <div class="path-text" title={proc.exe_path}>{proc.exe_path}</div>
          </div>

          <button class="attach-btn">
            <i class="fa-solid fa-link"></i> Attach
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="saved-list">
      {#each savedEntries as item (item.exe_path)}
        <div class="saved-row">
          <div class="saved-main">
            <div class="saved-name-row">
              <strong>{item.name}</strong>
              {#if item.saved_hook}
                <span class="hook-tag">
                  <i class="fa-solid fa-anchor"></i> {item.saved_hook.name} ({item.saved_hook.hookcode})
                </span>
              {:else}
                <span class="muted-tag">Auto-attach enabled (no pinned thread yet)</span>
              {/if}
            </div>
            <div class="saved-path">{item.exe_path}</div>
          </div>
          <button
            class="delete-btn"
            onclick={() => removeSavedGame(item.exe_path)}
            title="Remove from saved games"
          >
            <i class="fa-regular fa-trash-can"></i>
          </button>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .picker-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 1.25rem 1.5rem;
    gap: 1rem;
    overflow: hidden;
  }

  .picker-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 1rem;

    & h2 {
      font-size: 20px;
      display: flex;
      align-items: center;
      gap: 0.5rem;
      & i {
        color: var(--primary);
      }
    }

    & p {
      font-size: 13px;
      color: var(--secondary-text);
      margin-top: 0.2rem;
    }
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .tab-pills {
    display: flex;
    background: var(--accent);
    padding: 3px;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.08);

    & button {
      background: transparent;
      color: var(--secondary-text);
      border: none;
      padding: 0.4rem 0.85rem;
      border-radius: 6px;
      font-size: 12.5px;
      font-weight: 600;
      cursor: pointer;
      &.active {
        background: var(--primary);
        color: #111;
      }
    }
  }

  .refresh-btn {
    background: var(--accent);
    color: var(--main-text);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--small-radius);
    padding: 0.5rem 0.9rem;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 0.45rem;
    &:hover {
      border-color: var(--primary);
    }
  }

  .search-row {
    position: relative;
    display: flex;
    align-items: center;
    & i {
      position: absolute;
      left: 0.9rem;
      color: var(--secondary-text);
    }
    & input {
      width: 100%;
      background: var(--accent);
      border: 1px solid rgba(255, 255, 255, 0.08);
      border-radius: 8px;
      padding: 0.65rem 1rem 0.65rem 2.4rem;
      color: var(--main-text);
      font-size: 14px;
      &:focus {
        outline: 1px solid var(--primary);
      }
    }
  }

  .process-grid {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 0.75rem;
    align-content: start;
    padding-right: 4px;
  }

  .proc-card {
    background: var(--accent);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--big-radius);
    padding: 0.85rem 1rem;
    display: flex;
    align-items: center;
    gap: 0.85rem;
    cursor: pointer;
    transition: all 0.18s ease;

    &:hover {
      transform: translateY(-2px);
      border-color: var(--primary);
      box-shadow: 0 8px 20px rgba(0, 0, 0, 0.35);
    }

    &.saved {
      border-color: rgba(224, 175, 104, 0.35);
    }
  }

  .card-icon {
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: var(--main-background);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    color: var(--primary);
    font-size: 20px;

    & img {
      width: 30px;
      height: 30px;
      object-fit: contain;
    }
  }

  .card-info {
    flex: 1;
    min-width: 0;
  }

  .card-title-row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .win-title {
    font-weight: 700;
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .saved-pill {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(224, 175, 104, 0.2);
    color: #e0af68;
    flex-shrink: 0;
  }

  .knife-pill {
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(122, 162, 247, 0.2);
    color: #7aa2f7;
    flex-shrink: 0;
  }

  .exe-row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    margin-top: 2px;
    font-size: 12px;
  }

  .exe-name {
    color: var(--secondary);
    font-weight: 600;
  }

  .bit-badge {
    font-size: 10px;
    background: var(--main-background);
    padding: 1px 5px;
    border-radius: 4px;
    color: var(--secondary-text);
  }

  .pid-text {
    font-size: 11px;
    color: var(--secondary-text);
    font-family: monospace;
  }

  .path-text {
    font-size: 11px;
    color: var(--secondary-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: 2px;
  }

  .attach-btn {
    background: color-mix(in srgb, var(--primary) 20%, var(--main-background));
    color: var(--primary);
    border: 1px solid color-mix(in srgb, var(--primary) 40%, transparent);
    border-radius: var(--small-radius);
    padding: 0.4rem 0.75rem;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    flex-shrink: 0;
  }

  .saved-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .saved-row {
    background: var(--accent);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 8px;
    padding: 0.75rem 1rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }

  .saved-name-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .hook-tag {
    font-size: 11.5px;
    background: color-mix(in srgb, var(--primary) 16%, var(--main-background));
    color: var(--primary);
    padding: 2px 8px;
    border-radius: 4px;
    font-family: monospace;
  }

  .muted-tag {
    font-size: 11.5px;
    color: var(--secondary-text);
  }

  .saved-path {
    font-size: 12px;
    color: var(--secondary-text);
    margin-top: 3px;
  }

  .delete-btn {
    background: transparent;
    border: none;
    color: var(--secondary-text);
    cursor: pointer;
    font-size: 14px;
    padding: 6px;
    &:hover {
      color: #f7768e;
    }
  }
</style>
