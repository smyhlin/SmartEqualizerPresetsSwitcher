<script lang="ts">
  import { FileText, Info, Wrench } from '@lucide/svelte';

  import Button from '$lib/components/ui/button.svelte';

  type StatusTone = 'info' | 'success' | 'error';

  let {
    statusMessage = '',
    statusTone = 'info',
    busy = false,
    autorunEnabled = false,
    autorunLoaded = false,
    autorunBusy = false,
    onOpenLogs,
    onOpenTroubleshoot,
    onOpenAbout,
    onAutorunToggle,
    showTroubleshoot = true
  } = $props<{
    statusMessage?: string;
    statusTone?: StatusTone;
    busy?: boolean;
    autorunEnabled?: boolean;
    autorunLoaded?: boolean;
    autorunBusy?: boolean;
    onOpenLogs?: () => void;
    onOpenTroubleshoot?: () => void;
    onOpenAbout?: () => void;
    onAutorunToggle?: (event: Event) => void;
    showTroubleshoot?: boolean;
  }>();

  function openLogs() {
    onOpenLogs?.();
  }

  function openTroubleshoot() {
    onOpenTroubleshoot?.();
  }

  function openAbout() {
    onOpenAbout?.();
  }

  function changeAutorun(event: Event) {
    onAutorunToggle?.(event);
  }

  function toneClass() {
    if (statusTone === 'success') {
      return 'bg-success-soft text-success';
    }

    if (statusTone === 'error') {
      return 'bg-danger-soft text-danger';
    }

    return 'bg-accent-soft text-accent';
  }
</script>

<footer class="mt-4 flex h-[72px] shrink-0 items-center justify-between gap-4 overflow-hidden border-t border-border/80 bg-surface px-4 text-sm">
  <div class="flex min-w-0 flex-1 items-center gap-3">
    <span
      class={`inline-flex shrink-0 items-center px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.16em] ${toneClass()}`}
    >
      {busy ? 'WORKING' : statusTone.toUpperCase()}
    </span>

    <span class="min-w-0 truncate text-muted">{statusMessage}</span>
  </div>

  <div class="flex shrink-0 items-center gap-3">
    <Button
      variant="secondary"
      size="sm"
      class="min-w-[92px] justify-start rounded-none border-border/70 bg-surface-2 shadow-none hover:bg-surface-3"
      onclick={openLogs}
    >
      <FileText size={13} />
      Logs
    </Button>

    <label
      class={`inline-flex items-center gap-2 whitespace-nowrap leading-none ${
        autorunLoaded ? 'text-muted' : 'text-muted/70'
      }`}
      title="Launch the app automatically when you sign in"
    >
      <input
        type="checkbox"
        checked={autorunEnabled}
        disabled={!autorunLoaded || autorunBusy || busy}
        onchange={changeAutorun}
        class="focus-ring size-3.5 rounded-none border border-border bg-surface-2 accent-accent disabled:cursor-not-allowed disabled:opacity-60"
      />
      <span>Start with login</span>
    </label>
  </div>

  <div class="flex shrink-0 items-center gap-2">
    {#if showTroubleshoot}
      <Button
        variant="ghost"
        size="sm"
        class="min-w-[124px] justify-start rounded-none border border-border/70 bg-transparent text-foreground shadow-none hover:bg-surface-2 hover:text-foreground"
        onclick={openTroubleshoot}
      >
        <Wrench size={13} />
        Windows APO
      </Button>
    {/if}

    <Button
      variant="ghost"
      size="sm"
      class="min-w-[92px] justify-start rounded-none border border-border/70 bg-transparent text-foreground shadow-none hover:bg-surface-2 hover:text-foreground"
      onclick={openAbout}
    >
      <Info size={13} />
      About
    </Button>
  </div>
</footer>
