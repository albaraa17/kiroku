<script lang="ts">
  import SidebarButton from '$lib/components/SidebarButton.svelte';
  import { appStore } from '$lib/stores/app.svelte';

  let activeProc = $derived(appStore.snapshot.attached_processes[0] ?? null);
</script>

<nav>
  <section id="sidebar__header">
    <h1 title="LunaHook — Rust Bridge">鉤</h1>
    <div id="sidebar__header__buttons">
      <SidebarButton
        active={appStore.activeTab === 'console'}
        onclick={() => (appStore.activeTab = 'console')}
        tooltip="Live Hook Console"
        badge={appStore.snapshot.threads.length || null}
      >
        <i class="fa-solid fa-satellite-dish"></i>
      </SidebarButton>

      <SidebarButton
        active={appStore.activeTab === 'processes'}
        onclick={() => {
          appStore.activeTab = 'processes';
          appStore.refreshProcesses();
        }}
        tooltip="Attach Game / Process"
      >
        <i class="fa-solid fa-gamepad"></i>
      </SidebarButton>

      <SidebarButton
        active={appStore.activeTab === 'cleanup'}
        onclick={() => (appStore.activeTab = 'cleanup')}
        tooltip="Text Cleanup & Regex"
        badge={appStore.config?.regex_filters.filter((r) => r.enabled).length || null}
      >
        <i class="fa-solid fa-wand-magic-sparkles"></i>
      </SidebarButton>

      {#if activeProc}
        <div class="divider"></div>
        <SidebarButton
          active={false}
          onclick={() => (appStore.activeTab = 'console')}
          image={activeProc.icon_data_url}
          text={activeProc.icon_data_url ? undefined : activeProc.name[0]}
          tooltip={`Attached: ${activeProc.name} (PID ${activeProc.pid})`}
        />
      {/if}
    </div>
  </section>

  <section id="sidebar__footer">
    <SidebarButton
      active={appStore.activeTab === 'settings'}
      onclick={() => (appStore.activeTab = 'settings')}
      tooltip="Hook Engine & Settings"
    >
      <i class="fa-solid fa-gear"></i>
    </SidebarButton>
  </section>
</nav>

<style>
  nav {
    height: 100%;
    width: 85px;
    padding-bottom: 1rem;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: center;
    position: sticky;
    background-color: var(--accent);
    color: var(--main-text);
    border-right: 1px solid rgba(255, 255, 255, 0.05);
    box-shadow: 1px 0 10px rgba(0, 0, 0, 0.15);
    transition: background-color 0.3s ease;
    top: -1px;
    z-index: 3;
  }

  #sidebar__header {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 100%;

    h1 {
      color: var(--secondary);
      font-size: 46px;
      padding: 0.85rem 1rem 0.25rem;
      line-height: 1;
      text-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
    }

    #sidebar__header__buttons {
      text-align: center;
      margin-top: 2rem;
      display: flex;
      flex-direction: column;
      gap: 1rem;
      width: 100%;
    }
  }

  .divider {
    width: 36px;
    height: 1px;
    background: rgba(255, 255, 255, 0.1);
    margin: 0.25rem auto;
  }

  #sidebar__footer {
    margin-top: auto;
    width: 100%;
    display: flex;
    justify-content: center;
  }
</style>
