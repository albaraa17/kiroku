<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { appStore } from '$lib/stores/app.svelte';
  import type { RegexFilterRule, RegexReplaceRule } from '$lib/types';

  let sampleInput = $state(
    '【アルクェイド】ここんんににちちはは！<ruby>月姫<rt>つきひめ</rt></ruby>の世界へようこそ[r]ようこそようこそ',
  );
  let sampleOutput = $state('');

  let newFilterPattern = $state('');
  let newFilterProc = $state('');

  let newReplacePattern = $state('');
  let newReplaceTarget = $state('');

  async function updatePreview() {
    try {
      const activePath =
        appStore.snapshot.attached_processes[0]?.exe_path ?? null;
      sampleOutput = await invoke<string>('preview_clean_sentence', {
        raw: sampleInput,
        processPath: activePath,
      });
    } catch {
      sampleOutput = sampleInput;
    }
  }

  $effect(() => {
    void sampleInput;
    void appStore.config;
    updatePreview();
  });

  async function toggleFlag(
    key:
      | 'remove_repeated_chars'
      | 'remove_repeated_phrases'
      | 'strip_html_ruby_tags'
      | 'remove_speaker_brackets'
      | 'filter_chinese_segments'
      | 'normalize_whitespace',
  ) {
    if (!appStore.config) return;
    await appStore.saveConfig({
      ...appStore.config,
      [key]: !appStore.config[key],
    });
  }

  async function addRegexFilter(e: SubmitEvent) {
    e.preventDefault();
    if (!appStore.config || !newFilterPattern.trim()) return;
    const rule: RegexFilterRule = {
      id: `filter_${Date.now()}`,
      pattern: newFilterPattern.trim(),
      process_path: newFilterProc.trim() ? newFilterProc.trim() : null,
      enabled: true,
    };
    await appStore.saveConfig(
      {
        ...appStore.config,
        regex_filters: [rule, ...appStore.config.regex_filters],
      },
      true,
    );
    newFilterPattern = '';
    newFilterProc = '';
  }

  async function toggleRegexFilter(id: string) {
    if (!appStore.config) return;
    const updated = appStore.config.regex_filters.map((r) =>
      r.id === id ? { ...r, enabled: !r.enabled } : r,
    );
    await appStore.saveConfig({ ...appStore.config, regex_filters: updated });
  }

  async function deleteRegexFilter(id: string) {
    if (!appStore.config) return;
    const updated = appStore.config.regex_filters.filter((r) => r.id !== id);
    await appStore.saveConfig({ ...appStore.config, regex_filters: updated });
  }

  async function addRegexReplacement(e: SubmitEvent) {
    e.preventDefault();
    if (!appStore.config || !newReplacePattern.trim()) return;
    const rule: RegexReplaceRule = {
      id: `replace_${Date.now()}`,
      pattern: newReplacePattern.trim(),
      replacement: newReplaceTarget,
      ignore_case: false,
      process_path: null,
      enabled: true,
    };
    await appStore.saveConfig(
      {
        ...appStore.config,
        regex_replacements: [rule, ...appStore.config.regex_replacements],
      },
      true,
    );
    newReplacePattern = '';
    newReplaceTarget = '';
  }

  async function deleteRegexReplacement(id: string) {
    if (!appStore.config) return;
    const updated = appStore.config.regex_replacements.filter(
      (r) => r.id !== id,
    );
    await appStore.saveConfig({
      ...appStore.config,
      regex_replacements: updated,
    });
  }
</script>

<div class="cleanup-root">
  <header class="cleanup-header">
    <div>
      <h2><i class="fa-solid fa-wand-magic-sparkles"></i> Text Cleanup & Regex Pipeline</h2>
      <p>
        Native Rust replacement for Textractor's Repeated Characters, Repeated Phrases, Regex Filter, and Replacer extensions.
      </p>
    </div>
  </header>

  <!-- Live Test Sandbox -->
  <section class="sandbox-card">
    <div class="sandbox-col">
      <span class="field-label">Raw Input Test String</span>
      <input type="text" bind:value={sampleInput} />
    </div>
    <div class="arrow-icon">
      <i class="fa-solid fa-arrow-right"></i>
    </div>
    <div class="sandbox-col">
      <span class="field-label">Cleaned Output (Sent to exSTATic)</span>
      <div class="output-preview">{sampleOutput || '(Empty after filter)'}</div>
    </div>
  </section>

  {#if appStore.config}
    <div class="cleanup-grid">
      <!-- Left: Built-in Pipeline Toggles -->
      <section class="panel-card">
        <h3><i class="fa-solid fa-sliders"></i> Built-in Cleanup Stages</h3>

        <div class="toggle-list">
          <div class="toggle-row">
            <div>
              <strong>Remove Repeated Characters</strong>
              <p>Collapses doubled/tripled hook characters (e.g. <code>ここんんににちちはは</code> → <code>こんにちは</code>)</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.remove_repeated_chars}
                onchange={() => toggleFlag('remove_repeated_chars')}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="toggle-row">
            <div>
              <strong>Remove Repeated Phrases</strong>
              <p>Deduplicates echoed substrings (e.g. <code>おはようおはよう</code> → <code>おはよう</code>)</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.remove_repeated_phrases}
                onchange={() => toggleFlag('remove_repeated_phrases')}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="toggle-row">
            <div>
              <strong>Strip HTML, Ruby & Engine Tags</strong>
              <p>Strips furigana <code>&lt;rt&gt;...&lt;/rt&gt;</code>, Unity tags, and engine markers (<code>[r]</code>, <code>[n]</code>, <code>%0</code>)</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.strip_html_ruby_tags}
                onchange={() => toggleFlag('strip_html_ruby_tags')}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="toggle-row">
            <div>
              <strong>Strip Speaker Prefixes & Brackets</strong>
              <p>Removes speaker prefixes like <code>【アルク】</code> or <code>小鳥 「…」</code> before dialogue brackets</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.remove_speaker_brackets}
                onchange={() => toggleFlag('remove_speaker_brackets')}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="toggle-row">
            <div>
              <strong>Filter Dual-Language Chinese Segments</strong>
              <p>Strips Chinese translation clauses in bilingual VNs (e.g. <code>「おはよう！」早上好！</code> → <code>「おはよう！」</code>)</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.filter_chinese_segments}
                onchange={() => toggleFlag('filter_chinese_segments')}
              />
              <span class="slider"></span>
            </label>
          </div>

          <div class="toggle-row">
            <div>
              <strong>Normalize Whitespace & Empty Lines</strong>
              <p>Collapses consecutive spaces and blank lines</p>
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={appStore.config.normalize_whitespace}
                onchange={() => toggleFlag('normalize_whitespace')}
              />
              <span class="slider"></span>
            </label>
          </div>
        </div>

        <h3 style="margin-top: 1.25rem;">
          <i class="fa-solid fa-repeat"></i> Regex Replacements ({appStore.config.regex_replacements.length})
        </h3>
        <form class="add-rule-form" onsubmit={addRegexReplacement}>
          <input
            type="text"
            placeholder="Regex pattern..."
            bind:value={newReplacePattern}
          />
          <input
            type="text"
            placeholder="Replace with..."
            bind:value={newReplaceTarget}
          />
          <button type="submit"><i class="fa-solid fa-plus"></i> Add</button>
        </form>

        <div class="rules-scroll small">
          {#each appStore.config.regex_replacements as rule (rule.id)}
            <div class="rule-item">
              <code>/{rule.pattern}/ → "{rule.replacement}"</code>
              <button
                class="del-btn"
                onclick={() => deleteRegexReplacement(rule.id)}
                title="Delete rule"
              >
                <i class="fa-regular fa-trash-can"></i>
              </button>
            </div>
          {/each}
        </div>
      </section>

      <!-- Right: Regex Filter Rules (Imported from SavedRegexFilters.txt) -->
      <section class="panel-card">
        <h3>
          <i class="fa-solid fa-filter"></i> Regex Filters ({appStore.config.regex_filters.length})
        </h3>
        <p class="panel-sub">
          Any text matching an enabled regex pattern below is stripped out before broadcasting.
        </p>

        <form class="add-rule-form" onsubmit={addRegexFilter}>
          <input
            type="text"
            placeholder="Regex filter pattern (e.g. 【.*?】 or <[^>]+>)"
            bind:value={newFilterPattern}
          />
          <input
            type="text"
            placeholder="Optional .exe name (blank = Global)"
            bind:value={newFilterProc}
          />
          <button type="submit"><i class="fa-solid fa-plus"></i> Add</button>
        </form>

        <div class="rules-scroll">
          {#each appStore.config.regex_filters as rule (rule.id)}
            <div class="rule-item" class:disabled={!rule.enabled}>
              <div class="rule-info">
                <code class="rule-pat">{rule.pattern}</code>
                <span class="scope-badge">
                  {rule.process_path
                    ? rule.process_path.split('\\').pop()
                    : 'Global'}
                </span>
              </div>
              <div class="rule-actions">
                <label class="switch">
                  <input
                    type="checkbox"
                    checked={rule.enabled}
                    onchange={() => toggleRegexFilter(rule.id)}
                  />
                  <span class="slider"></span>
                </label>
                <button
                  class="del-btn"
                  onclick={() => deleteRegexFilter(rule.id)}
                  title="Delete filter"
                >
                  <i class="fa-regular fa-trash-can"></i>
                </button>
              </div>
            </div>
          {/each}
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .cleanup-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: 1.25rem 1.5rem;
    gap: 1rem;
    overflow: hidden;
  }

  .cleanup-header h2 {
    font-size: 20px;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    & i {
      color: var(--primary);
    }
  }

  .cleanup-header p {
    font-size: 13px;
    color: var(--secondary-text);
    margin-top: 0.2rem;
  }

  .sandbox-card {
    background: var(--accent);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: var(--big-radius);
    padding: 0.85rem 1.1rem;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 1rem;
  }

  .sandbox-col {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    & .field-label {
      font-size: 11px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--secondary-text);
    }
    & input {
      background: var(--main-background);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 6px;
      padding: 0.5rem 0.75rem;
      color: var(--main-text);
      font-size: 13.5px;
    }
  }

  .arrow-icon {
    color: var(--primary);
    font-size: 18px;
    padding-top: 1rem;
  }

  .output-preview {
    background: color-mix(in srgb, var(--primary) 10%, var(--main-background));
    border: 1px solid color-mix(in srgb, var(--primary) 35%, transparent);
    border-radius: 6px;
    padding: 0.5rem 0.75rem;
    font-size: 14px;
    color: var(--main-text);
    min-height: 36px;
    display: flex;
    align-items: center;
  }

  .cleanup-grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }

  .panel-card {
    background: var(--accent);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: var(--big-radius);
    padding: 1rem 1.15rem;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;

    & h3 {
      font-size: 15px;
      display: flex;
      align-items: center;
      gap: 0.5rem;
      margin-bottom: 0.75rem;
      & i {
        color: var(--primary);
      }
    }
  }

  .panel-sub {
    font-size: 12px;
    color: var(--secondary-text);
    margin-bottom: 0.75rem;
  }

  .toggle-list {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .toggle-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    background: var(--main-background);
    padding: 0.65rem 0.85rem;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.05);

    & strong {
      font-size: 13.5px;
    }
    & p {
      font-size: 11.5px;
      color: var(--secondary-text);
      margin-top: 2px;
    }
  }

  .add-rule-form {
    display: flex;
    gap: 0.45rem;
    margin-bottom: 0.75rem;
    & input {
      flex: 1;
      background: var(--main-background);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 6px;
      padding: 0.45rem 0.65rem;
      color: var(--main-text);
      font-size: 12.5px;
    }
    & button {
      background: var(--primary);
      color: #111;
      border: none;
      border-radius: 6px;
      padding: 0 0.85rem;
      font-weight: 700;
      font-size: 12.5px;
      cursor: pointer;
    }
  }

  .rules-scroll {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    &.small {
      max-height: 130px;
    }
  }

  .rule-item {
    background: var(--main-background);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
    padding: 0.55rem 0.75rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    &.disabled {
      opacity: 0.5;
    }
  }

  .rule-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    flex-wrap: wrap;
  }

  .rule-pat {
    font-family: monospace;
    font-size: 12.5px;
    color: var(--primary);
    background: rgba(255, 255, 255, 0.04);
    padding: 2px 6px;
    border-radius: 4px;
  }

  .scope-badge {
    font-size: 11px;
    color: var(--secondary-text);
    background: var(--accent);
    padding: 2px 7px;
    border-radius: 99px;
  }

  .rule-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .del-btn {
    background: transparent;
    border: none;
    color: var(--secondary-text);
    cursor: pointer;
    padding: 4px;
    &:hover {
      color: #f7768e;
    }
  }
</style>
