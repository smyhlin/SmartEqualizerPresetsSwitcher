<script lang="ts">
  import { onMount } from 'svelte';
  import {
    CheckCircle2,
    CircleAlert,
    Copy,
    Download,
    RefreshCw,
    Search,
    X
  } from '@lucide/svelte';

  import { analyzeAutoEqPreset } from '$lib/autoeq';
  import Button from '$lib/components/ui/button.svelte';
  import Input from '$lib/components/ui/input.svelte';
  import {
    getAutoEqPresetVariant,
    loadAutoEqIndex,
    onAutoEqProgress
  } from '$lib/tauri';
  import type {
    AutoEqIndexEntry,
    AutoEqPresetVariant,
    AutoEqPresetAnalysis,
    AutoEqProgressPayload,
    AutoEqProgressSource,
    EqBackendStatus
  } from '$lib/types';
  import { cn, sanitizeImportName } from '$lib/utils';

  type StatusTone = 'info' | 'success' | 'warn' | 'error';
  type BadgeTone = 'accent' | 'neutral' | 'success' | 'warning';
  type ModalStatus = {
    message: string;
    tone: StatusTone;
    source: AutoEqProgressSource | null;
    busy: boolean;
  };


  type VariantOption = {
    value: AutoEqPresetVariant;
    label: string;
    description: string;
    badge: string;
  };

  const variantOptions: VariantOption[] = [
    {
      value: 'auto',
      label: 'Auto target',
      description: 'Use the best variant for the detected backend: ParametricEQ when available, GraphicEQ fallback with Linux PipeWire conversion.',
      badge: 'Recommended'
    },
    {
      value: 'parametric',
      label: 'ParametricEQ / Filter',
      description: 'Best for PipeWire filter-chain, EasyEffects-style PEQ workflows, Equalizer APO and Peace. If unavailable, use Auto target so GraphicEQ can be converted.',
      badge: 'System EQ'
    },
    {
      value: 'graphic',
      label: 'GraphicEQ',
      description: 'Fallback for simple graphic equalizers and manual editing.',
      badge: 'Fallback'
    }
  ];


  let {
    open = false,
    targetGroupName = null,
    eqBackendStatus = null,
    onClose,
    onImport,
  } = $props<{
    open?: boolean;
    targetGroupName?: string | null;
    eqBackendStatus?: EqBackendStatus | null;
    onClose?: () => void;
    onImport?: (value: { entry: AutoEqIndexEntry; presetText: string }) => Promise<boolean> | boolean;
  }>();

  const emptyAnalysis: AutoEqPresetAnalysis = {
    kind: 'Unknown',
    filterCount: 0,
    hasPreamp: false,
    hasInclude: false,
    hasDevice: false,
    hasChannel: false,
    hasConvolution: false,
    hasGraphicEq: false
  };

  let entries = $state<AutoEqIndexEntry[]>([]);
  let searchTerm = $state('');
  let selected = $state<AutoEqIndexEntry | null>(null);
  let presetText = $state('');
  let indexLoading = $state(false);
  let previewLoading = $state(false);
  let importing = $state(false);
  let indexLoaded = $state(false);
  let shouldLoadIndex = $state(true);
  let indexError = $state<string | null>(null);
  let previewError = $state<string | null>(null);
  let indexProgress = $state<AutoEqProgressPayload | null>(null);
  let previewProgress = $state<AutoEqProgressPayload | null>(null);
  let previewRequestToken = 0;
  let selectedVariant = $state<AutoEqPresetVariant>('auto');

  const filteredEntries = $derived.by(() => {
    const term = searchTerm.toLowerCase().trim();
    if (!term) {
      return entries;
    }

    return entries.filter((entry) => {
      return entry.n.toLowerCase().includes(term) || entry.s.toLowerCase().includes(term);
    });
  });

  const visibleEntries = $derived.by(() => filteredEntries.slice(0, 200));
  const importLabel = $derived(
    targetGroupName ? `Import to ${targetGroupName}` : 'Import to Imported group'
  );
  const selectedVariantOption = $derived(
    variantOptions.find((option) => option.value === selectedVariant) ?? variantOptions[0]
  );
  const backendTargetSummary = $derived.by(() => {
    if (!eqBackendStatus) {
      return 'Auto target prefers ParametricEQ first, then falls back to GraphicEQ.';
    }

    if (eqBackendStatus.platform === 'linux') {
      return `${eqBackendStatus.backendName}: Auto target prefers ParametricEQ/Filter for PipeWire or EasyEffects, then falls back to GraphicEQ.`;
    }

    if (eqBackendStatus.platform === 'windows') {
      return `${eqBackendStatus.backendName}: Auto target prefers ParametricEQ/Filter for Equalizer APO/Peace, then falls back to GraphicEQ.`;
    }

    return 'Auto target picks the safest available AutoEQ text variant.';
  });

  const previewReady = $derived(
    !previewLoading && !previewError && presetText.trim().length > 0
  );
  const canImport = $derived(
    Boolean(selected) && previewReady && !importing
  );
  const selectedAnalysis = $derived.by(() => {
    if (!previewReady) {
      return emptyAnalysis;
    }

    return analyzeAutoEqPreset(presetText);
  });
  const selectedBadges = $derived.by(() => {
    const analysis = selectedAnalysis;
    const badges: Array<{ label: string; tone: BadgeTone }> = [];

    if (analysis.kind !== 'Unknown') {
      badges.push({
        label: analysis.kind,
        tone: analysis.kind === 'GraphicEQ' ? 'accent' : 'neutral'
      });
    }

    if (analysis.filterCount > 0) {
      badges.push({
        label: `${analysis.filterCount} filters`,
        tone: 'neutral'
      });
    }

    if (analysis.hasPreamp) {
      badges.push({ label: 'Preamp', tone: 'success' });
    }

    if (analysis.hasInclude) {
      badges.push({ label: 'Include', tone: 'warning' });
    }

    if (analysis.hasDevice) {
      badges.push({ label: 'Device', tone: 'warning' });
    }

    if (analysis.hasChannel) {
      badges.push({ label: 'Channel', tone: 'warning' });
    }

    if (analysis.hasConvolution) {
      badges.push({ label: 'Convolution', tone: 'warning' });
    }

    return badges;
  });
  const indexStatus = $derived.by(() => {
    if (indexError) {
      return {
        message: indexError,
        tone: 'error',
        source: indexProgress?.source ?? null,
        busy: false
      } satisfies ModalStatus;
    }

    if (indexLoading) {
      return {
        message: indexProgress?.message ?? 'Preparing AutoEQ index.',
        tone: 'info',
        source: indexProgress?.source ?? null,
        busy: true
      } satisfies ModalStatus;
    }

    if (
      indexProgress &&
      (indexProgress.phase === 'done' || indexProgress.phase === 'cache-hit')
    ) {
      return {
        message: indexProgress.message,
        tone: toneFromSource(indexProgress.source ?? null),
        source: indexProgress.source ?? null,
        busy: false
      } satisfies ModalStatus;
    }

    if (indexLoaded) {
      return {
        message: 'AutoEQ index ready.',
        tone: 'success',
        source: null,
        busy: false
      } satisfies ModalStatus;
    }

    return {
      message: 'Search becomes available as soon as the AutoEQ index is ready.',
      tone: 'info',
      source: null,
      busy: false
    } satisfies ModalStatus;
  });
  const previewStatus = $derived.by(() => {
    if (!selected) {
      return {
        message: 'Select a result to preview the best AutoEQ variant for the selected backend target.',
        tone: 'info',
        source: null,
        busy: false
      } satisfies ModalStatus;
    }

    if (previewError) {
      return {
        message: previewError,
        tone: 'error',
        source: previewProgress?.source ?? null,
        busy: false
      } satisfies ModalStatus;
    }

    if (previewLoading) {
      return {
        message: previewProgress?.message ?? 'Preparing AutoEQ preset.',
        tone: 'info',
        source: previewProgress?.source ?? null,
        busy: true
      } satisfies ModalStatus;
    }

    if (
      previewProgress &&
      (previewProgress.phase === 'done' || previewProgress.phase === 'cache-hit')
    ) {
      return {
        message: previewProgress.message,
        tone: toneFromSource(previewProgress.source ?? null),
        source: previewProgress.source ?? null,
        busy: false
      } satisfies ModalStatus;
    }

    if (previewReady) {
      return {
        message: 'AutoEQ preset ready.',
        tone: 'success',
        source: null,
        busy: false
      } satisfies ModalStatus;
    }

    return {
      message: 'Select a result to load the selected AutoEQ variant.',
      tone: 'info',
      source: null,
      busy: false
    } satisfies ModalStatus;
  });

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    void onAutoEqProgress((payload) => {
      if (payload.operation === 'index') {
        indexProgress = payload;
        return;
      }

      if (
        selected &&
        payload.presetName === selected.n &&
        payload.presetSource === selected.s
      ) {
        previewProgress = payload;
      }
    })
      .then((value) => {
        if (disposed) {
          value();
          return;
        }
        unlisten = value;
      })
      .catch((error) => {
        indexError = getErrorMessage(error);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  $effect(() => {
    if (open && shouldLoadIndex && !indexLoading) {
      shouldLoadIndex = false;
      void loadIndex();
    }
  });

  function getErrorMessage(error: unknown) {
    if (typeof error === 'string') {
      return error;
    }

    if (
      error &&
      typeof error === 'object' &&
      'message' in error &&
      typeof error.message === 'string'
    ) {
      return error.message;
    }

    return 'An unexpected error occurred.';
  }

  function toneFromSource(source: AutoEqProgressSource | null): StatusTone {
    if (source === 'stale-cache') {
      return 'warn';
    }

    if (source === 'cache' || source === 'network') {
      return 'success';
    }

    return 'info';
  }

  function sourceLabel(source: AutoEqProgressSource | null) {
    if (source === 'cache') {
      return 'Cache';
    }

    if (source === 'network') {
      return 'Network';
    }

    if (source === 'stale-cache') {
      return 'Stale cache';
    }

    return null;
  }

  function statusClasses(tone: StatusTone) {
    return cn(
      'flex items-center justify-between gap-3 rounded-[12px] border px-3 py-2 text-[11px]',
      tone === 'error'
        ? 'border-danger/35 bg-danger-soft text-foreground'
        : tone === 'warn'
          ? 'border-amber-500/30 bg-amber-500/10 text-foreground'
          : tone === 'success'
            ? 'border-success/25 bg-success-soft text-foreground'
            : 'border-border bg-surface text-foreground'
    );
  }

  function badgeClasses(tone: BadgeTone) {
    return cn(
      'inline-flex items-center rounded-full border px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.08em]',
      tone === 'accent'
        ? 'border-accent/35 bg-accent-soft text-accent'
        : tone === 'success'
          ? 'border-success/25 bg-success-soft text-success'
          : tone === 'warning'
            ? 'border-amber-500/25 bg-amber-500/10 text-amber-300'
            : 'border-border bg-surface text-muted'
    );
  }

  async function loadIndex(forceRefresh = false) {
    indexLoading = true;
    indexError = null;
    indexProgress = {
      operation: 'index',
      phase: 'start',
      message: forceRefresh ? 'Refreshing AutoEQ index.' : 'Preparing AutoEQ index.',
      source: null,
      presetName: null,
      presetSource: null
    };

    try {
      entries = await loadAutoEqIndex(forceRefresh);
      indexLoaded = true;
    } catch (error) {
      indexLoaded = false;
      indexError = getErrorMessage(error);
    } finally {
      indexLoading = false;
    }
  }

  async function selectPreset(entry: AutoEqIndexEntry) {
    const requestToken = ++previewRequestToken;

    selected = entry;
    presetText = '';
    previewError = null;
    previewLoading = true;
    previewProgress = {
      operation: 'preset',
      phase: 'start',
      message: 'Preparing AutoEQ preset. First preview may download the AutoEQ package archive.',
      source: null,
      presetName: entry.n,
      presetSource: entry.s
    };

    try {
      const content = await getAutoEqPresetVariant(entry.n, entry.s, selectedVariant);
      if (requestToken !== previewRequestToken) {
        return;
      }

      presetText = content;
    } catch (error) {
      if (requestToken !== previewRequestToken) {
        return;
      }

      const firstError = getErrorMessage(error);
      previewProgress = {
        operation: 'preset',
        phase: 'download-archive',
        message: `First attempt failed (${firstError}). Retrying once with cache fallback...`,
        source: null,
        presetName: entry.n,
        presetSource: entry.s
      };

      try {
        const retryContent = await getAutoEqPresetVariant(entry.n, entry.s, selectedVariant);
        if (requestToken !== previewRequestToken) {
          return;
        }
        presetText = retryContent;
        previewError = null;
      } catch (retryError) {
        if (requestToken !== previewRequestToken) {
          return;
        }
        previewError = getErrorMessage(retryError);
      }
    } finally {
      if (requestToken === previewRequestToken) {
        previewLoading = false;
      }
    }
  }

  async function copyToClipboard() {
    if (!previewReady || !navigator?.clipboard) {
      return;
    }

    await navigator.clipboard.writeText(presetText);
  }

  function downloadPreset() {
    if (!selected || !previewReady) {
      return;
    }

    const filename = sanitizeImportName(
      `${selected.n} (${selected.s}) ${selectedVariantOption.label}`,
      'AutoEQ Preset'
    );
    const blob = new Blob([presetText], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `${filename}.txt`;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  async function importSelectedPreset() {
    if (!selected || !canImport) {
      return;
    }

    importing = true;
    try {
      const imported = (await onImport?.({ entry: selected, presetText })) ?? false;
      if (imported) {
        close();
      }
    } finally {
      importing = false;
    }
  }

  function close() {
    previewRequestToken += 1;
    previewLoading = false;

    if (!indexLoaded) {
      shouldLoadIndex = true;
    }

    onClose?.();
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (open && event.key === 'Escape') {
      close();
    }
  }}
/>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-4">
    <button
      type="button"
      class="absolute inset-0 z-0 bg-black/65 backdrop-blur-[2px]"
      aria-label="Close AutoEQ dialog"
      onclick={close}
    ></button>

    <div class="shell-surface relative z-10 flex h-[680px] w-full max-w-[1080px] flex-col overflow-hidden rounded-[24px] shadow-[0_28px_80px_rgba(0,0,0,0.55)]">
      <div class="border-b border-border px-5 py-4">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <div class="text-sm font-semibold text-foreground">Import from AutoEQ</div>
            <div class="mt-0.5 text-xs text-muted">
              Search the packaged AutoEQ index, auto-pick ParametricEQ/GraphicEQ by backend, and import the right variant into the current workflow.
            </div>
          </div>

          <Button variant="ghost" size="icon" onclick={close} ariaLabel="Close AutoEQ dialog">
            <X size={16} />
          </Button>
        </div>
      </div>

      <div class="grid min-h-0 flex-1 gap-4 p-5 xl:grid-cols-[420px_minmax(0,1fr)]">
        <div class="flex min-h-0 flex-col gap-3">
          <div class="shell-surface-2 rounded-[14px] px-3 py-2">
            <div class="flex items-center gap-2">
              <Search size={16} class="shrink-0 text-muted" />
              <Input
                bind:value={searchTerm}
                placeholder="Search headphones, IEMs, or source names"
                class="border-0 bg-transparent px-0 shadow-none focus-visible:ring-0"
              />
              <Button
                size="icon"
                variant="ghost"
                ariaLabel="Refresh AutoEQ index"
                title="Refresh AutoEQ index"
                onclick={() => void loadIndex(true)}
                disabled={indexLoading}
                class="size-8 shrink-0 rounded-[8px]"
              >
                <RefreshCw size={14} class={indexLoading ? 'animate-spin' : ''} />
              </Button>
            </div>
          </div>

          <div class="shell-surface-2 rounded-[14px] px-3 py-2">
            <label class="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted" for="autoeq-variant">
              Target EQ app
            </label>
            <select
              id="autoeq-variant"
              bind:value={selectedVariant}
              onchange={() => {
                if (selected && !previewLoading) {
                  void selectPreset(selected);
                }
              }}
              class="mt-2 w-full rounded-[10px] border border-border bg-surface px-3 py-2 text-sm text-foreground outline-none focus:border-accent"
            >
              {#each variantOptions as option}
                <option value={option.value}>{option.label} — {option.badge}</option>
              {/each}
            </select>
            <p class="mt-2 text-[11px] leading-5 text-muted">{selectedVariantOption.description}</p>
            <p class="mt-1 text-[11px] leading-5 text-muted/80">{backendTargetSummary}</p>
          </div>

          <div class={statusClasses(indexStatus.tone)}>
            <div class="flex min-w-0 items-center gap-2">
              {#if indexStatus.tone === 'error'}
                <CircleAlert size={14} class="shrink-0 text-danger" />
              {:else if indexStatus.busy}
                <RefreshCw size={14} class="shrink-0 animate-spin text-accent" />
              {:else}
                <CheckCircle2 size={14} class="shrink-0 text-success" />
              {/if}
              <span class="truncate">{indexStatus.message}</span>
            </div>

            {#if sourceLabel(indexStatus.source)}
              <span class="shrink-0 rounded-full bg-background/50 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted">
                {sourceLabel(indexStatus.source)}
              </span>
            {/if}
          </div>

          <div class="shell-surface-2 flex items-center justify-between rounded-[14px] px-4 py-3 text-xs text-muted">
            <span>Results {filteredEntries.length}</span>
            <span>
              {#if filteredEntries.length > visibleEntries.length}
                Showing first {visibleEntries.length}
              {:else}
                Ready to inspect visible results
              {/if}
            </span>
          </div>

          <div class="shell-surface-2 min-h-0 flex-1 overflow-hidden rounded-[18px]">
            {#if indexLoading && entries.length === 0}
              <div class="h-full overflow-y-auto p-2">
                {#each Array.from({ length: 7 }) as _, index}
                  <div
                    class="mb-2 animate-pulse rounded-[14px] border border-border bg-surface px-4 py-3"
                    aria-hidden="true"
                  >
                    <div class="h-3 w-2/3 rounded bg-white/8"></div>
                    <div class="mt-2 h-2.5 w-1/2 rounded bg-white/6"></div>
                    <div class="mt-3 flex gap-2">
                      <div class="h-5 w-18 rounded-full bg-white/6"></div>
                      <div class="h-5 w-12 rounded-full bg-white/6"></div>
                    </div>
                  </div>
                {/each}
              </div>
            {:else if entries.length === 0}
              <div class="flex h-full items-center justify-center px-4 text-sm text-muted">
                No AutoEQ index entries are available yet.
              </div>
            {:else}
              <div class="h-full overflow-y-auto p-2">
                {#if filteredEntries.length === 0}
                  <div class="rounded-[14px] border border-dashed border-border px-4 py-5 text-sm text-muted">
                    No presets match the current search. Try a model name, alias, or source like
                    `oratory1990`.
                  </div>
                {/if}

                {#each visibleEntries as preset (preset.i)}
                  <button
                    type="button"
                    onclick={() => void selectPreset(preset)}
                    class={cn(
                      'mb-2 flex w-full items-start justify-between gap-3 rounded-[14px] border px-4 py-3 text-left transition-colors',
                      selected?.i === preset.i
                        ? 'border-accent/45 bg-accent/10 shadow-[0_0_0_1px_rgba(132,204,22,0.12)]'
                        : 'border-border bg-surface hover:bg-surface-3'
                    )}
                  >
                    <span class="min-w-0 flex-1">
                      <span class="block truncate text-sm font-semibold text-foreground">
                        {preset.n}
                      </span>
                      <span class="mt-1 block truncate text-xs text-muted">
                        {preset.s} • ID {preset.i}
                      </span>
                      <span class="mt-2 flex flex-wrap gap-1.5">
                        <span class={badgeClasses('neutral')}>Auto variants</span>
                        {#if preset.r > 0}
                          <span class={badgeClasses('success')}>Rank {preset.r}</span>
                        {/if}
                      </span>
                    </span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        </div>

        <div class="flex min-h-0 flex-col gap-3">
          {#if selected}
            <div class="shell-surface-2 rounded-[18px] px-4 py-4">
              <div class="flex items-start justify-between gap-4">
                <div class="min-w-0">
                  <div class="truncate text-lg font-semibold text-foreground">{selected.n}</div>
                  <div class="mt-1 text-xs text-muted">{selected.s} • ID {selected.i}</div>
                </div>

                <span class="rounded-full bg-surface px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted">
                  {importLabel}
                </span>
              </div>
            </div>

            <div class={statusClasses(previewStatus.tone)}>
              <div class="flex min-w-0 items-center gap-2">
                {#if previewStatus.tone === 'error'}
                  <CircleAlert size={14} class="shrink-0 text-danger" />
                {:else if previewStatus.busy}
                  <RefreshCw size={14} class="shrink-0 animate-spin text-accent" />
                {:else}
                  <CheckCircle2 size={14} class="shrink-0 text-success" />
                {/if}
                <span class="truncate">{previewStatus.message}</span>
              </div>

              {#if sourceLabel(previewStatus.source)}
                <span class="shrink-0 rounded-full bg-background/50 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted">
                  {sourceLabel(previewStatus.source)}
                </span>
              {/if}
            </div>

            <div class="shell-surface-2 min-h-0 flex-1 overflow-hidden rounded-[18px]">
              <div class="border-b border-border px-4 py-3">
                <div class="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted">
                  Preset summary
                </div>
                <div class="mt-2 flex flex-wrap gap-2">
                  {#if selectedBadges.length > 0}
                    {#each selectedBadges as badge}
                      <span class={badgeClasses(badge.tone)}>{badge.label}</span>
                    {/each}
                  {:else}
                    <span class={badgeClasses('neutral')}>Awaiting preset analysis</span>
                  {/if}
                </div>
              </div>

              {#if previewLoading}
                <div class="space-y-3 p-4">
                  <div class="h-5 w-32 animate-pulse rounded bg-white/8"></div>
                  <div class="rounded-[16px] border border-border bg-[#050a0f] p-4">
                    {#each Array.from({ length: 10 }) as _, index}
                      <div
                        class="mb-2 h-3 animate-pulse rounded bg-white/6"
                        style={`width: ${90 - index * 5}%`}
                      ></div>
                    {/each}
                  </div>
                </div>
              {:else if previewError}
                <div class="flex h-full items-center justify-center px-5 text-sm text-foreground">
                  <div class="rounded-[14px] border border-danger/40 bg-danger-soft px-4 py-4">
                    {previewError}
                  </div>
                </div>
              {:else if previewReady}
                <div class="min-h-0 flex-1 overflow-auto bg-[#050a0f]">
                  <pre class="px-4 py-4 font-mono text-[12px] leading-6 text-[#dce6f5]">{presetText}</pre>
                </div>
              {:else}
                <div class="flex h-full items-center justify-center px-6 text-center text-sm text-muted">
                  Select a result to load the selected AutoEQ variant.
                </div>
              {/if}
            </div>

            <div class="flex items-center justify-between gap-3">
              <div class="flex gap-2">
                <Button
                  variant="secondary"
                  onclick={() => void copyToClipboard()}
                  disabled={!previewReady}
                >
                  <Copy size={14} />
                  Copy
                </Button>
                <Button
                  variant="secondary"
                  onclick={downloadPreset}
                  disabled={!previewReady}
                >
                  <Download size={14} />
                  Download .txt
                </Button>
              </div>

              <div class="text-right text-[11px] text-muted">
                Import remains the primary action once the preview is ready.
              </div>
            </div>
          {:else}
            <div class="shell-surface-2 flex min-h-0 flex-1 items-center justify-center rounded-[18px] px-6 text-center text-sm text-muted">
              Select an AutoEQ result to preview the target variant, verify tags like
              `GraphicEQ`, `Filters`, `Preamp`, `Include`, `Device`, `Channel`, or
              `Convolution`, and then import it into the current group.
            </div>
          {/if}
        </div>
      </div>

      <div class="border-t border-border px-5 py-4">
        <div class="flex items-center justify-between gap-3">
          <div class="text-[11px] text-muted">
First preview may download the AutoEQ package archive; later previews use the local cache.
          </div>

          <div class="flex items-center gap-3">
            <Button variant="ghost" onclick={close}>Cancel</Button>
            <Button onclick={() => void importSelectedPreset()} disabled={!canImport}>
              {#if importing}
                Importing...
              {:else}
                {importLabel}
              {/if}
            </Button>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}
