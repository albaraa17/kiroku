<script lang="ts">
  import '@fontsource-variable/noto-sans-jp';
  import '@fortawesome/fontawesome-free/css/all.min.css';
  import '../app.css';
  import { useWindowTitlebar } from '$lib/composables/useWindowTitlebar.svelte';
  import { onMount, onDestroy, type Snippet } from 'svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { Toaster } from 'svelte-sonner';
  import { appStore } from '$lib/stores/app.svelte';

  const { children }: { children: Snippet } = $props();

  useWindowTitlebar();

  onMount(async () => {
    await appStore.init();
  });

  onDestroy(() => {
    appStore.destroy();
  });
</script>

<main>
  <Sidebar />
  <div data-tauri-drag-region class="titlebar">
    <div data-tauri-drag-region class="titlebar-info">
      <span class="brand-pill">KIROKU 記録</span>
      <span class="status-dot" class:online={appStore.snapshot.ws_clients > 0}></span>
      <span class="status-label">
        exSTATic WS :{appStore.snapshot.ws_port} ({appStore.snapshot.ws_clients} client{appStore.snapshot.ws_clients === 1 ? '' : 's'})
      </span>
      {#if appStore.snapshot.knife_connected}
        <span class="knife-pill">
          <i class="fa-solid fa-bolt"></i> Knife WS Connected
        </span>
      {/if}
    </div>
    <div class="titlebar-actions">
      <div class="titlebar-button" id="titlebar-minimize">
        <img src="minimize.svg" alt="minimize" />
      </div>
      <div class="titlebar-button" id="titlebar-maximize">
        <img src="maximize.svg" alt="maximize" />
      </div>
      <div class="titlebar-button" id="titlebar-close">
        <img src="close.svg" alt="close" />
      </div>
    </div>
  </div>
  <div class="content">
    {@render children()}
  </div>
  <Toaster
    position="top-right"
    offset={{
      top: 36,
    }}
    visibleToasts={2}
    toastOptions={{
      style:
        'background: var(--accent); color: var(--main-text); border: 1px solid rgba(255, 255, 255, 0.1); font-family: "Noto Sans JP Variable", sans-serif;',
    }}
  />
</main>

<style>
  main {
    display: grid;
    grid-template-columns: 85px 1fr;
    grid-template-rows: 30px 1fr;
    grid-template-areas:
      'sidebar titlebar'
      'sidebar content';
    height: 100vh;
    width: 100vw;
    overflow: hidden;
  }

  :global(nav) {
    grid-area: sidebar;
  }

  .titlebar {
    grid-area: titlebar;
  }

  .titlebar-info {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding-left: 1rem;
    font-size: 12px;
    color: var(--secondary-text);
    pointer-events: none;
  }

  .brand-pill {
    font-weight: 700;
    letter-spacing: 0.08em;
    color: var(--secondary);
    font-size: 11px;
  }

  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #6b7280;
    transition: background 0.2s ease;
    &.online {
      background: #9ece6a;
      box-shadow: 0 0 8px #9ece6a;
    }
  }

  .knife-pill {
    background: rgba(122, 162, 247, 0.15);
    color: #7aa2f7;
    border: 1px solid rgba(122, 162, 247, 0.3);
    padding: 1px 8px;
    border-radius: 99px;
    font-size: 11px;
    font-weight: 600;
  }

  .titlebar-actions {
    display: flex;
    align-items: center;
  }

  .content {
    grid-area: content;
    overflow: hidden;
    position: relative;
  }
</style>
