import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'svelte-sonner';
import { applyTheme } from '$lib/theme';
import type {
  AppConfig,
  DashboardSnapshot,
  NavTab,
  ProcessEntry,
} from '$lib/types';

class AppStateStore {
  activeTab = $state<NavTab>('console');
  snapshot = $state<DashboardSnapshot>({
    attached_processes: [],
    threads: [],
    selected_thread_key: null,
    ws_port: 9001,
    ws_clients: 0,
    knife_connected: false,
    knife_url: 'ws://127.0.0.1:6677',
    console_logs: [],
    total_session_chars: 0,
    total_session_sentences: 0,
  });
  config = $state<AppConfig | null>(null);
  processes = $state<ProcessEntry[]>([]);
  loadingProcesses = $state(false);

  private unlistenDashboard: UnlistenFn | null = null;

  async init() {
    try {
      this.unlistenDashboard = await listen<DashboardSnapshot>(
        'dashboard_updated',
        (event) => {
          this.snapshot = event.payload;
        },
      );

      const [cfg, snap] = await Promise.all([
        invoke<AppConfig>('get_config'),
        invoke<DashboardSnapshot>('get_dashboard_snapshot'),
      ]);
      this.config = cfg;
      this.snapshot = snap;
      applyTheme(cfg.theme_settings);
    } catch (err) {
      console.error('Failed to initialize AppStateStore:', err);
    }
  }

  destroy() {
    if (this.unlistenDashboard) {
      this.unlistenDashboard();
      this.unlistenDashboard = null;
    }
  }

  async refreshProcesses() {
    this.loadingProcesses = true;
    try {
      this.processes = await invoke<ProcessEntry[]>('list_processes');
    } catch (err) {
      toast.error(`Failed to list processes: ${err}`);
    } finally {
      this.loadingProcesses = false;
    }
  }

  async attachProcess(pid: number) {
    try {
      this.snapshot = await invoke<DashboardSnapshot>('attach_process', { pid });
      this.config = await invoke<AppConfig>('get_config');
      toast.success(`Attached to PID ${pid}`);
      this.activeTab = 'console';
    } catch (err) {
      toast.error(`Attach failed: ${err}`);
    }
  }

  async detachProcess(pid: number) {
    try {
      this.snapshot = await invoke<DashboardSnapshot>('detach_process', { pid });
      toast.info(`Detached from PID ${pid}`);
    } catch (err) {
      toast.error(`Detach failed: ${err}`);
    }
  }

  async detachAll() {
    try {
      this.snapshot = await invoke<DashboardSnapshot>('detach_all');
      toast.info('Detached all processes');
    } catch (err) {
      toast.error(`Detach failed: ${err}`);
    }
  }

  async selectThread(key: string | null, remember = true) {
    try {
      this.snapshot = await invoke<DashboardSnapshot>('select_thread', {
        key,
        remember,
      });
      if (remember && key) {
        this.config = await invoke<AppConfig>('get_config');
      }
    } catch (err) {
      toast.error(`Failed to select thread: ${err}`);
    }
  }

  async insertHookCode(hookcode: string) {
    if (!hookcode.trim()) return;
    try {
      this.snapshot = await invoke<DashboardSnapshot>('insert_hook_code', {
        hookcode: hookcode.trim(),
      });
      toast.success(`Inserted hook code: ${hookcode.trim()}`);
    } catch (err) {
      toast.error(`Failed to insert hook: ${err}`);
    }
  }

  async insertPcHooks(which = 0) {
    try {
      this.snapshot = await invoke<DashboardSnapshot>('insert_pc_hooks', {
        which,
      });
      toast.success('Inserted standard PC / GDI hooks');
    } catch (err) {
      toast.error(`Failed to insert PC hooks: ${err}`);
    }
  }

  async removeHook(pid: number, addr: number) {
    try {
      this.snapshot = await invoke<DashboardSnapshot>('remove_hook', {
        pid,
        addr,
      });
      toast.info('Removed hook');
    } catch (err) {
      toast.error(`Failed to remove hook: ${err}`);
    }
  }

  async saveConfig(newConfig: AppConfig, showToast = false) {
    try {
      const saved = await invoke<AppConfig>('save_config', {
        config: newConfig,
      });
      this.config = saved;
      applyTheme(saved.theme_settings);
      if (showToast) {
        toast.success('Settings saved & applied to LunaHost');
      }
    } catch (err) {
      toast.error(`Failed to save settings: ${err}`);
    }
  }

  async copyText(text: string, label = 'Copied to clipboard') {
    try {
      await invoke('copy_to_clipboard', { text });
      toast.success(label);
    } catch (err) {
      toast.error(`Clipboard error: ${err}`);
    }
  }

  async broadcastTestSentence(sentence: string) {
    try {
      const count = await invoke<number>('broadcast_test_sentence', {
        sentence,
      });
      toast.success(
        `Broadcast test sentence to ${count} connected exSTATic client(s)`,
      );
    } catch (err) {
      toast.error(`Broadcast failed: ${err}`);
    }
  }
}

export const appStore = new AppStateStore();
