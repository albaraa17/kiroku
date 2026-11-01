<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { invoke } from '@tauri-apps/api/core';
  import { toast } from 'svelte-sonner';
  import { appStore } from '$lib/stores/app.svelte';
  import { CODEPAGE_OPTIONS, COLOR_SWATCHES, THEMES } from '$lib/constants';
  import type { AppConfig } from '$lib/types';

  let testSentence = $state('「これはテスト文です。exSTATicへ送信します。」');

  async function updateConfigField<K extends keyof AppConfig>(
    key: K,
    value: AppConfig[K],
    notify = true,
  ) {
    if (!appStore.config) return;
    await appStore.saveConfig({ ...appStore.config, [key]: value }, notify);
  }

  async function selectTheme(themeId: string) {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      theme_settings: {
        ...appStore.config.theme_settings,
        theme: themeId,
        use_custom_color: false,
      },
    });
  }

  async function selectAccentColor(color: string) {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      theme_settings: {
        ...appStore.config.theme_settings,
        accent_color: color,
        use_custom_color: true,
      },
    });
  }

  async function handleImportLegacyFile() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'LunaHost / Textractor Config',
            extensions: ['json', 'txt'],
          },
        ],
      });
      if (!selected || typeof selected !== 'string') return;
      const updated = await invoke<AppConfig>('import_legacy_config_file', {
        path: selected,
      });
      appStore.config = updated;
      toast.success('Successfully imported legacy hooks & filters!');
    } catch (err) {
      toast.error(`Import failed: ${err}`);
    }
  }
</script>

<div class="settings-root">
  <header class="settings-header">
    <div>
      <h2><i class="fa-solid fa-gear"></i> Hook Engine, WebSocket & Theme Settings</h2>
      <p>
        Customize LunaHost hooking parameters (flush delay, codepage, buffers), exSTATic / Knife WebSocket ports, and Tadoku themes.
      </p>
    </div>

    <button class="import-btn" onclick={handleImportLegacyFile}>
      <i class="fa-solid fa-file-import"></i> Import config64.json / SavedRegexFilters.txt
    </button>
  </header>

  {#if appStore.config}
    <div class="settings-scroll">
      <!-- 1. Customizable LunaHost Engine Settings -->
      <section class="settings-card">
        <h3><i class="fa-solid fa-microchip"></i> LunaHost Hook Engine Parameters</h3>

        <div class="form-grid">
          <div class="field-box">
            <label for="flush-delay-input">
              Flush Delay (ms)
              <span class="hint">Time to wait before flushing hook buffer (`TextThread::flushDelay`)</span>
            </label>
            <div class="slider-row">
              <input
                type="range"
                min="10"
                max="1000"
                step="10"
                value={appStore.config.flush_delay}
                onchange={(e) =>
                  updateConfigField(
                    'flush_delay',
                    parseInt((e.currentTarget as HTMLInputElement).value, 10),
                  )}
              />
              <input
                id="flush-delay-input"
                type="number"
                min="0"
                max="5000"
                step="10"
                value={appStore.config.flush_delay}
                onchange={(e) =>
                  updateConfigField(
                    'flush_delay',
                    parseInt((e.currentTarget as HTMLInputElement).value, 10),
                  )}
                class="num-input"
              />
            </div>
          </div>

          <div class="field-box">
            <label for="codepage-select">
              Default Codepage (`Host::defaultCodepage`)
              <span class="hint">Fallback encoding for non-Unicode game hooks (932 = Shift-JIS)</span>
            </label>
            <div class="slider-row">
              <select
                id="codepage-select"
                value={appStore.config.default_codepage}
                onchange={(e) =>
                  updateConfigField(
                    'default_codepage',
                    parseInt((e.currentTarget as HTMLSelectElement).value, 10),
                  )}
              >
                {#each CODEPAGE_OPTIONS as opt (opt.value)}
                  <option value={opt.value}>{opt.label}</option>
                {/each}
              </select>
              <input
                type="number"
                min="0"
                max="65001"
                value={appStore.config.default_codepage}
                onchange={(e) =>
                  updateConfigField(
                    'default_codepage',
                    parseInt((e.currentTarget as HTMLInputElement).value, 10),
                  )}
                class="num-input"
              />
            </div>
          </div>

          <div class="field-box">
            <label for="max-buf-input">
              Max Buffer Size (chars)
              <span class="hint">Maximum characters buffered before forced flush (`maxBufferSize`)</span>
            </label>
            <input
              id="max-buf-input"
              type="number"
              min="100"
              max="100000"
              step="100"
              value={appStore.config.max_buffer_size}
              onchange={(e) =>
                updateConfigField(
                  'max_buffer_size',
                  parseInt((e.currentTarget as HTMLInputElement).value, 10),
                )}
            />
          </div>

          <div class="field-box">
            <label for="max-hist-input">
              Max History Size (chars)
              <span class="hint">Internal LunaHost thread history limit (`maxHistorySize`)</span>
            </label>
            <input
              id="max-hist-input"
              type="number"
              min="10000"
              max="100000000"
              step="100000"
              value={appStore.config.max_history_size}
              onchange={(e) =>
                updateConfigField(
                  'max_history_size',
                  parseInt((e.currentTarget as HTMLInputElement).value, 10),
                )}
            />
          </div>
        </div>

        <div class="switches-grid">
          <div class="switch-card">
            <div>
              <strong>Enable PC / GDI Hooks on Attach</strong>
              <p>Automatically enable standard Windows text hooks (`Host::enablePCHooks`)</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.enable_pc_hooks}
                onchange={() =>
                  updateConfigField(
                    'enable_pc_hooks',
                    !appStore.config!.enable_pc_hooks,
                  )}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="switch-card">
            <div>
              <strong>Auto-Attach Running Games</strong>
              <p>Automatically attach when a saved visual novel or `knife.exe` is detected</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.auto_attach}
                onchange={() =>
                  updateConfigField('auto_attach', !appStore.config!.auto_attach)}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="switch-card">
            <div>
              <strong>Auto-Attach Only Games with Saved Hook</strong>
              <p>Only auto-attach to games in `saved_hooks` that already have a known hook</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.auto_attach_saved_only}
                onchange={() =>
                  updateConfigField(
                    'auto_attach_saved_only',
                    !appStore.config!.auto_attach_saved_only,
                  )}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="switch-card">
            <div>
              <strong>Minimize to System Tray on Close</strong>
              <p>Keep the Rust hook + WebSocket bridge running in tray when closing the window</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.minimize_to_tray}
                onchange={() =>
                  updateConfigField(
                    'minimize_to_tray',
                    !appStore.config!.minimize_to_tray,
                  )}
              />
              <span class="slider"></span>
            </label>
          </div>
        </div>
      </section>

      <!-- 2. exSTATic Broadcast & Knife WebSocket Settings -->
      <section class="settings-card">
        <h3><i class="fa-solid fa-tower-broadcast"></i> exSTATic Output & Knife WebSocket Ingest</h3>

        <div class="form-grid">
          <div class="field-box">
            <label for="ws-port-input">
              exSTATic Broadcast WebSocket Port
              <span class="hint">Broadcasts <code>&#123; process_id, process_path, sentence &#125;</code> (Default: 9001)</span>
            </label>
            <div class="slider-row">
              <input
                id="ws-port-input"
                type="number"
                min="1024"
                max="65535"
                value={appStore.config.ws_broadcast_port}
                onchange={(e) =>
                  updateConfigField(
                    'ws_broadcast_port',
                    parseInt((e.currentTarget as HTMLInputElement).value, 10),
                  )}
              />
              <label class="switch">
                <input
                  type="checkbox"
                  checked={appStore.config.ws_broadcast_enabled}
                  onchange={() =>
                    updateConfigField(
                      'ws_broadcast_enabled',
                      !appStore.config!.ws_broadcast_enabled,
                    )}
                />
                <span class="slider"></span>
              </label>
            </div>
          </div>

          <div class="field-box">
            <label for="knife-url-input">
              Knife / External WebSocket Input URL
              <span class="hint">Connects to <code>knife --text-websocket</code> (e.g. <code>ws://127.0.0.1:6677</code>)</span>
            </label>
            <div class="slider-row">
              <input
                id="knife-url-input"
                type="text"
                value={appStore.config.knife_ws_url}
                onchange={(e) =>
                  updateConfigField(
                    'knife_ws_url',
                    (e.currentTarget as HTMLInputElement).value,
                  )}
              />
              <label class="switch">
                <input
                  type="checkbox"
                  checked={appStore.config.knife_ws_enabled}
                  onchange={() =>
                    updateConfigField(
                      'knife_ws_enabled',
                      !appStore.config!.knife_ws_enabled,
                    )}
                />
                <span class="slider"></span>
              </label>
            </div>
          </div>
        </div>

        <div class="test-broadcast-bar">
          <input
            type="text"
            bind:value={testSentence}
            placeholder="Test sentence to broadcast to exSTATic..."
          />
          <button
            class="primary-btn"
            onclick={() => appStore.broadcastTestSentence(testSentence)}
          >
            <i class="fa-solid fa-paper-plane"></i> Send Test Payload to exSTATic
          </button>
        </div>
      </section>

      <!-- 3. Tadoku Themes & Custom Accent Swatches -->
      <section class="settings-card">
        <h3><i class="fa-solid fa-palette"></i> Tadoku Theme & Accent Colors</h3>

        <div class="themes-row">
          {#each THEMES as t (t.id)}
            <button
              class="theme-chip"
              class:active={appStore.config.theme_settings.theme === t.id &&
                !appStore.config.theme_settings.use_custom_color}
              onclick={() => selectTheme(t.id)}
              style={`--chip-bg: ${t.background}; --chip-accent: ${t.accent}; --chip-primary: ${t.primary};`}
            >
              <span class="color-dot"></span>
              <span>{t.name}</span>
            </button>
          {/each}
        </div>

        <div class="swatches-row">
          <span class="swatch-label">Custom Accent Swatch:</span>
          {#each COLOR_SWATCHES as color (color)}
            <button
              class="swatch-btn"
              class:active={appStore.config.theme_settings.use_custom_color &&
                appStore.config.theme_settings.accent_color === color}
              style={`background-color: ${color};`}
              onclick={() => selectAccentColor(color)}
              title={color}
            ></button>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .settings-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 1.25rem 1.5rem;
    gap: 1rem;
    overflow: hidden;
  }

  .settings-header {
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

  .import-btn {
    background: var(--accent);
    color: var(--primary);
    border: 1px solid color-mix(in srgb, var(--primary) 40%, transparent);
    border-radius: var(--small-radius);
    padding: 0.55rem 1rem;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    &:hover {
      background: color-mix(in srgb, var(--primary) 15%, var(--accent));
    }
  }

  .settings-scroll {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding-right: 4px;
  }

  .settings-card {
    background: var(--accent);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--big-radius);
    padding: 1.1rem 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;

    & h3 {
      font-size: 15px;
      display: flex;
      align-items: center;
      gap: 0.5rem;
      & i {
        color: var(--primary);
      }
    }
  }

  .form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 1rem;
  }

  .field-box {
    background: var(--main-background);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 8px;
    padding: 0.75rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;

    & label {
      font-size: 13px;
      font-weight: 600;
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    & .hint {
      font-size: 11.5px;
      font-weight: 400;
      color: var(--secondary-text);
    }

    & input[type='number'],
    & input[type='text'],
    & select {
      background: var(--accent);
      color: var(--main-text);
      border: 1px solid rgba(255, 255, 255, 0.12);
      border-radius: 6px;
      padding: 0.45rem 0.65rem;
      font-size: 13px;
      width: 100%;
    }
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    & input[type='range'] {
      flex: 1;
      accent-color: var(--primary);
    }
    & .num-input {
      width: 90px !important;
    }
  }

  .switches-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 0.75rem;
  }

  .switch-card {
    background: var(--main-background);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 0.7rem 0.9rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;

    & strong {
      font-size: 13px;
    }
    & p {
      font-size: 11.5px;
      color: var(--secondary-text);
      margin-top: 2px;
    }
  }

  .test-broadcast-bar {
    display: flex;
    gap: 0.6rem;
    & input {
      flex: 1;
      background: var(--main-background);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 6px;
      padding: 0.5rem 0.75rem;
      color: var(--main-text);
      font-size: 13px;
    }
  }

  .primary-btn {
    background: var(--primary);
    color: #111;
    border: none;
    border-radius: 6px;
    padding: 0.5rem 1rem;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.45rem;
  }

  .themes-row {
    display: flex;
    gap: 0.65rem;
    flex-wrap: wrap;
  }

  .theme-chip {
    background: var(--chip-bg);
    color: #fff;
    border: 2px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    padding: 0.6rem 1rem;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s ease;

    & .color-dot {
      width: 14px;
      height: 14px;
      border-radius: 50%;
      background: var(--chip-primary);
    }

    &.active {
      border-color: var(--chip-primary);
      box-shadow: 0 0 12px rgba(187, 154, 247, 0.3);
    }
  }

  .swatches-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }

  .swatch-label {
    font-size: 12.5px;
    color: var(--secondary-text);
    margin-right: 0.25rem;
  }

  .swatch-btn {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    transition: transform 0.15s ease;
    &:hover {
      transform: scale(1.15);
    }
    &.active {
      border-color: #fff;
      transform: scale(1.15);
    }
  }
</style>
