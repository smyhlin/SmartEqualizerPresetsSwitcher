<script lang="ts">
  import { Save, X } from '@lucide/svelte';

  import Button from '$lib/components/ui/button.svelte';
  import ConvolutionFilePanel from '$lib/components/ConvolutionFilePanel.svelte';
  import type { PresetConvolution } from '$lib/types';

  let {
    open = false,
    groupName = null,
    presetName = null,
    presetFilePath = null,
    panelKey = '',
    presetConvolution = null,
    draft = '',
    dirty = false,
    configPath = null,
    configTargetLabel = 'Backend config',
    onDraftChange,
    onSave,
    onClose,
    onToggleConvolution,
  } = $props<{
    open?: boolean;
    groupName?: string | null;
    presetName?: string | null;
    presetFilePath?: string | null;
    panelKey?: string;
    presetConvolution?: PresetConvolution | null;
    draft?: string;
    dirty?: boolean;
    configPath?: string | null;
    configTargetLabel?: string;
    onDraftChange?: (value: string) => void;
    onSave?: () => void;
    onClose?: () => void;
    onToggleConvolution?: (value: {
      groupName: string;
      presetName: string;
      enabled: boolean;
    }) => Promise<boolean> | boolean;
  }>();

  let localValue = $state('');
  let editorElement = $state<HTMLTextAreaElement | null>(null);
  let activeKey = '';

  $effect(() => {
    const nextKey = open ? `${groupName ?? ''}::${presetName ?? ''}::${presetFilePath ?? ''}` : '';

    if (!open) {
      activeKey = '';
      return;
    }

    if (nextKey !== activeKey) {
      localValue = draft;
      queueMicrotask(() => {
        editorElement?.focus();
        editorElement?.setSelectionRange(editorElement.value.length, editorElement.value.length);
      });
      activeKey = nextKey;
      return;
    }

    if (localValue !== draft) {
      localValue = draft;
    }

    activeKey = nextKey;
  });

  function updateDraft(value: string) {
    localValue = value;
    onDraftChange?.(value);
  }

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

  function close() {
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
      aria-label="Close config editor"
      onclick={close}
    ></button>

    <div class="shell-surface relative z-10 flex h-[min(92vh,980px)] w-full max-w-[1200px] flex-col overflow-hidden rounded-[18px] shadow-[0_28px_80px_rgba(0,0,0,0.55)]">
      <div class="border-b border-border px-4 py-3">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <div class="text-sm font-medium text-foreground">Edit config</div>
            <div class="mt-0.5 truncate text-xs text-muted">
              {#if groupName && presetName}
                {groupName} / {presetName}
              {:else}
                Select a preset to edit
              {/if}
            </div>
            {#if presetFilePath}
              <div class="mt-1 truncate text-[11px] text-muted">
                File: <span class="font-mono text-foreground">{presetFilePath}</span>
              </div>
            {/if}
          </div>

          <div class="flex items-center gap-2">
            <Button variant="secondary" onclick={() => { onSave?.(); close(); }}>
              <Save size={14} />
              Save File
            </Button>
            <Button variant="ghost" size="icon" onclick={close} ariaLabel="Close editor">
              <X size={16} />
            </Button>
          </div>
        </div>
      </div>



      <div class="border-b border-border px-4 py-2 text-xs text-muted">
        <div class="flex flex-wrap items-center gap-2">
          <span>Writing active output to</span>
          <span class="rounded-full bg-surface-3 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-foreground/70">
            {configTargetLabel}
          </span>
          <span class="font-mono text-foreground">{configPath}</span>
        </div>
      </div>

      <div class="border-b border-border px-4 py-3">
        <ConvolutionFilePanel
          draft={localValue}
          {configPath}
          {panelKey}
          presetError={presetConvolution?.error ?? null}
          onToggleConvolution={handleToggleConvolution}
        />
      </div>

      <div class="flex min-h-0 flex-1 p-4">
        <div class="flex min-h-0 flex-1 overflow-hidden rounded-[14px] border border-accent/20 bg-[#08131b] shadow-[inset_0_0_0_1px_rgba(132,204,22,0.05)]">
          <textarea
            bind:this={editorElement}
            bind:value={localValue}
            rows="1"
            placeholder="// Edit the preset text here"
            spellcheck="false"
            autocomplete="off"
            autocapitalize="off"
            wrap="soft"
            class="h-full min-h-0 flex-1 resize-none rounded-none border-0 bg-transparent px-4 py-4 font-mono text-[13px] leading-6 text-[#dce6f5] caret-[#84cc16] shadow-none outline-none placeholder:text-[#6f8094] focus:outline-none"
            oninput={() => updateDraft(localValue)}
          ></textarea>
        </div>
      </div>

      <div class="border-t border-border px-4 py-2 text-xs text-muted">
        {#if dirty}
          Unsaved changes
        {:else}
          Saved to preset storage
        {/if}
        <span class="mx-2 text-border">•</span>
        Close or save to commit your changes
      </div>
    </div>
  </div>
{/if}
