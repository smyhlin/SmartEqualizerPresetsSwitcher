<script lang="ts">
  import {
    Download,
    Pencil,
    Save,
    WandSparkles,
  } from "@lucide/svelte";

  import Button from "$lib/components/ui/button.svelte";
  import ConvolutionFilePanel from '$lib/components/ConvolutionFilePanel.svelte';
  import type { PresetConvolution } from '$lib/types';

  let {
    groupName = null,
    presetName = null,
    presetFilePath = null,
    configPath = null,
    panelKey = '',
    presetConvolution = null,
    draft = "",
    dirty = false,
    onSave,
    onApply,
    onExport,
    onEditConfig,
    onToggleConvolution,
  } = $props<{
    groupName?: string | null;
    presetName?: string | null;
    presetFilePath?: string | null;
    configPath?: string | null;
    panelKey?: string;
    presetConvolution?: PresetConvolution | null;
    draft?: string;
    dirty?: boolean;
    onSave?: () => void;
    onApply?: () => void;
    onExport?: () => void;
    onEditConfig?: () => void;
    onToggleConvolution?: (value: {
      groupName: string;
      presetName: string;
      enabled: boolean;
    }) => Promise<boolean> | boolean;
  }>();

  function handleToggleConvolution(value: { enabled: boolean }) {
    if (!groupName || !presetName) {
      return false;
    }

    return (
      onToggleConvolution?.({
        groupName,
        presetName,
        enabled: value.enabled
      }) ?? false
    );
  }
</script>

<section class="flex h-full min-h-0 flex-col overflow-hidden bg-surface">
  <div class="border-b border-border bg-gradient-to-b from-surface-2/30 to-surface">
    <div class="px-6 py-6 flex flex-col gap-6">
      
      <!-- Top Row: Status & Actions -->
      <div class="flex items-center justify-between gap-4">
      <!-- Status -->
      <div class={`flex h-5 items-center justify-center gap-1.5 rounded-full px-2.5 text-[9px] font-black uppercase tracking-wider ring-1 ring-inset transition-all duration-300 ${dirty ? 'bg-yellow-500/10 text-yellow-500 ring-yellow-500/30' : 'bg-green-500/10 text-green-500 ring-green-500/30'}`}>
          <div class={`h-1.5 w-1.5 rounded-full shadow-[0_0_8px_rgba(0,0,0,0.2)] ${dirty ? 'animate-pulse bg-yellow-400' : 'bg-green-400'}`}></div>
          {dirty ? 'Unsaved' : 'Synced'}
        </div>

        <!-- Toolbar -->
        {#if groupName && presetName}
          <div class="flex shrink-0 items-center gap-2">
            <Button variant="outline" size="sm" class="h-8 gap-1.5 border-border/60 bg-background/50 text-[11px] font-semibold" onclick={() => onEditConfig?.()}>
              <Pencil size={14} /> Edit
            </Button>
            <Button variant="outline" size="sm" class="h-8 gap-1.5 border-border/60 bg-background/50 text-[11px] font-semibold" onclick={() => onExport?.()}>
              <Download size={14} /> Export
            </Button>
            <div class="mx-1 h-4 w-px bg-border/80"></div>
            <Button variant="secondary" size="sm" class="h-8 gap-1.5 bg-surface-3 text-[11px] font-semibold" onclick={() => onSave?.()}>
              <Save size={14} /> Save
            </Button>
            <Button size="sm" class="h-8 gap-1.5 text-[11px] font-bold" onclick={() => onApply?.()}>
              <WandSparkles size={14} /> Apply
            </Button>
          </div>
        {/if}
      </div>

      <!-- Bottom Row: Multi-line / Full-width Title -->
      <div class="min-w-0">
        {#if groupName && presetName}
          <div class="text-[10px] font-bold text-muted/50 uppercase tracking-[0.12em] mb-0.5">{groupName}</div>
          <h1 class="truncate text-lg lg:text-xl font-bold tracking-tight text-foreground leading-snug" title={presetName}>
            {presetName}
          </h1>
        {:else}
          <h1 class="text-xl font-bold tracking-tight text-muted/40">Select Preset</h1>
        {/if}
      </div>

    </div>
  </div>

  {#if groupName && presetName}
    <div class="border-b border-border/60 px-6 py-4">
      <ConvolutionFilePanel
        {draft}
        {configPath}
        {panelKey}
        presetError={presetConvolution?.error ?? null}
        onToggleConvolution={handleToggleConvolution}
      />
    </div>
  {/if}





  <!-- Code Preview Pane -->
  <div class="flex min-h-0 flex-1 bg-surface-2/20 p-5">
    <div class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-white/[0.03] bg-[#050a0f] shadow-2xl ring-1 ring-white/5">
      <!-- Preview Header -->
      <div class="flex items-center justify-between border-b border-white/[0.05] bg-white/[0.02] px-4 py-2">
        <div class="flex items-center gap-2">
          <div class="flex gap-1.5">
            <div class="h-2.5 w-2.5 rounded-full bg-white/10"></div>
            <div class="h-2.5 w-2.5 rounded-full bg-white/10"></div>
            <div class="h-2.5 w-2.5 rounded-full bg-white/10"></div>
          </div>
          <span class="ml-2 text-[10px] font-bold uppercase tracking-widest text-white/30">Preview</span>
        </div>

      </div>

      <!-- Preview Content -->
      {#if groupName && presetName}
        <div class="relative min-h-0 flex-1 overflow-auto p-5 font-mono text-[13px] leading-relaxed text-blue-100/80 [scrollbar-gutter:stable]">
          <pre class="whitespace-pre">{draft || '// No preset contents found'}</pre>
        </div>
      {:else}
        <div class="flex flex-1 flex-col items-center justify-center gap-4 text-muted/30">
          <div class="rounded-full bg-white/[0.02] p-6 ring-1 ring-white/[0.05]">
             <WandSparkles size={32} class="opacity-20" />
          </div>
          <p class="text-[11px] font-bold uppercase tracking-[0.2em]">Select a preset to begin</p>
        </div>
      {/if}
    </div>
  </div>

  <!-- Minimalist Status Footer -->
  <div class="border-t border-border bg-surface px-6 py-2.5">
    <div class="flex items-center justify-between text-[10px] font-bold uppercase tracking-[0.14em]">
      <div class="flex items-center gap-2 text-muted/50">
        <div class={`h-1 w-1 rounded-full ${dirty ? 'bg-warning' : 'bg-success'}`}></div>
        {dirty ? 'Changes pending save' : 'All changes in storage'}
      </div>
      <div class="text-muted/40 font-bold">
        Backend Auto-Reload Enabled
      </div>
    </div>
  </div>
</section>
