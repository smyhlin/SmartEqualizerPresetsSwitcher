<script lang="ts">
  import { ExternalLink, Info, X } from '@lucide/svelte';

  import Button from '$lib/components/ui/button.svelte';

  let { open = false, onClose, onOpenRepository } = $props<{
    open?: boolean;
    onClose?: () => void;
    onOpenRepository?: () => Promise<unknown> | unknown;
  }>();

  const description =
    'SmartEQPresetSwitcher is a cross-platform desktop app for organizing, editing, applying, importing, exporting, and backing up EQ presets. It supports Equalizer APO on Windows and Linux EQ export workflows such as PipeWire. It is built with SvelteKit, TypeScript, Rust, and Tauri 2.';
  const repositoryUrl = 'https://github.com/smyhlin/SmartEQPresetSwitcher';

  function close() {
    onClose?.();
  }

  function openRepository(event: MouseEvent) {
    event.preventDefault();
    void onOpenRepository?.();
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
      aria-label="Close about dialog"
      onclick={close}
    ></button>

    <div class="shell-surface relative z-10 flex w-full max-w-[760px] flex-col overflow-hidden rounded-[18px] shadow-[0_28px_80px_rgba(0,0,0,0.55)]">
      <div class="border-b border-border px-4 py-3">
        <div class="flex items-start justify-between gap-4">
          <div class="flex min-w-0 items-center gap-3">
            <div class="flex size-10 shrink-0 items-center justify-center rounded-[12px] border border-accent/30 bg-accent-soft text-accent">
              <Info size={18} />
            </div>
            <div class="min-w-0">
              <div class="text-sm font-semibold text-foreground">About SmartEQPresetSwitcher</div>
              <div class="mt-0.5 text-xs text-muted">Project details and source reference</div>
            </div>
          </div>

          <Button variant="ghost" size="icon" onclick={close} ariaLabel="Close about dialog">
            <X size={16} />
          </Button>
        </div>
      </div>

      <div class="space-y-4 px-4 py-4">
        <div class="shell-surface-2 rounded-[14px] border border-border px-4 py-4">
          <p class="text-sm leading-6 text-foreground/90">{description}</p>
        </div>

        <div class="grid gap-3">
          <div class="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted">Repository</div>
          <a
            href={repositoryUrl}
            rel="noreferrer noopener"
            onclick={openRepository}
            class="shell-surface-2 group flex items-center justify-between gap-3 rounded-[14px] border border-border px-4 py-3 text-left transition-colors hover:border-accent/35 hover:bg-surface-3/70"
          >
            <span class="min-w-0 break-all font-mono text-[12px] leading-5 text-accent select-text underline decoration-accent/30 decoration-dotted underline-offset-4">
              {repositoryUrl}
            </span>
            <span class="inline-flex shrink-0 items-center gap-1 text-[11px] font-medium uppercase tracking-[0.14em] text-muted transition group-hover:text-accent">
              Open in browser
              <ExternalLink size={12} />
            </span>
          </a>
        </div>
      </div>

      <div class="border-t border-border px-4 py-3">
        <div class="flex items-center justify-between gap-3">
          <p class="text-xs leading-5 text-muted">
            The repository link is selectable and opens in the default browser.
          </p>
          <Button variant="secondary" onclick={close}>Close</Button>
        </div>
      </div>
    </div>
  </div>
{/if}
