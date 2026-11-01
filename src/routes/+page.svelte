<script lang="ts">
  import { appStore } from '$lib/stores/app.svelte';
  import HookConsoleView from '$lib/components/HookConsoleView.svelte';
  import ProcessPickerView from '$lib/components/ProcessPickerView.svelte';
  import CleanupPipelineView from '$lib/components/CleanupPipelineView.svelte';
  import SettingsView from '$lib/components/SettingsView.svelte';
</script>

<div class="container">
  {#if appStore.activeTab === 'console'}
    <HookConsoleView />
  {:else if appStore.activeTab === 'processes'}
    <ProcessPickerView />
  {:else if appStore.activeTab === 'cleanup'}
    <CleanupPipelineView />
  {:else if appStore.activeTab === 'settings'}
    <SettingsView />
  {/if}
</div>
