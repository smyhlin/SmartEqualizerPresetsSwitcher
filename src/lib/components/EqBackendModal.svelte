<script lang="ts">
  import { CheckCircle2, CircleAlert, CircleDot, ExternalLink, Info, Loader2, Wrench } from '@lucide/svelte';

  import Button from '$lib/components/ui/button.svelte';
  import type { EqBackendStatus } from '$lib/types';

  let {
    open = false,
    status = null,
    busy = false,
    windowsToolsAvailable = false,
    onClose,
    onRefresh,
    onSetup,
    onOpenTroubleshoot,
    onOpenPath
  } = $props<{
    open?: boolean;
    status?: EqBackendStatus | null;
    busy?: boolean;
    windowsToolsAvailable?: boolean;
    onClose?: () => void;
    onRefresh?: () => void;
    onSetup?: () => void;
    onOpenTroubleshoot?: () => void;
    onOpenPath?: (path: string) => void;
  }>();

  function stateClass(state?: string) {
    if (state === 'connected' || state === 'export_ready') {
      return 'border-success/40 bg-success-soft text-success';
    }

    if (state === 'setup_needed' || state === 'no_active_preset') {
      return 'border-amber-400/35 bg-amber-500/10 text-amber-200';
    }

    return 'border-border bg-surface-2 text-muted';
  }

  function stateIcon(state?: string) {
    if (state === 'connected' || state === 'export_ready') {
      return CheckCircle2;
    }

    if (state === 'setup_needed' || state === 'no_active_preset') {
      return CircleAlert;
    }

    return Info;
  }

  async function copyText(value?: string | null) {
    if (!value) {
      return;
    }

    try {
      await navigator.clipboard.writeText(value);
    } catch {
      // Clipboard access is best-effort only.
    }
  }
</script>

{#if open}
  {@const Icon = stateIcon(status?.state)}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-5 backdrop-blur-sm">
    <section class="shell-surface flex max-h-[88vh] w-full max-w-5xl flex-col overflow-hidden shadow-[0_24px_80px_rgba(0,0,0,0.55)]">
      <header class="flex items-start justify-between gap-4 border-b border-border/80 px-6 py-5">
        <div class="min-w-0">
          <h2 class="text-xl font-semibold text-foreground">EQ backend status</h2>
          <p class="mt-1 text-sm text-muted">
            Shows whether SmartEQPresetSwitcher is only managing presets locally, or whether the OS audio backend is ready to use them.
          </p>
        </div>
        <button
          class="focus-ring rounded-none px-2 py-1 text-muted transition hover:text-foreground"
          type="button"
          onclick={onClose}
          aria-label="Close EQ backend status"
        >
          ×
        </button>
      </header>

      <div class="grid min-h-0 gap-4 overflow-y-auto p-6 lg:grid-cols-[1.05fr_1fr]">
        <section class={`rounded-[18px] border p-5 ${stateClass(status?.state)}`}>
          <div class="flex items-center gap-3">
            <Icon size={24} />
            <div class="min-w-0">
              <p class="text-[11px] font-semibold uppercase tracking-[0.18em] opacity-80">
                {status?.backendName ?? 'Detecting backend'}
              </p>
              <h3 class="mt-1 text-2xl font-semibold text-foreground">
                {status?.statusLabel ?? 'Checking EQ backend...'}
              </h3>
            </div>
          </div>

          <p class="mt-4 text-sm leading-6 text-muted">
            {status?.statusDetail ?? 'Backend status is loading.'}
          </p>

          <div class="mt-5 grid gap-3 text-sm">
            <div class="rounded-[14px] border border-border/70 bg-surface/70 p-3">
              <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Active preset</p>
              <p class="mt-1 truncate text-foreground">
                {status?.activeGroupName && status?.activePresetName
                  ? `${status.activeGroupName} / ${status.activePresetName}`
                  : 'None'}
              </p>
            </div>

            {#if status?.configPath}
              <div class="rounded-[14px] border border-border/70 bg-surface/70 p-3">
                <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Managed config</p>
                <p class="mt-1 truncate font-mono text-xs text-foreground">{status.configPath}</p>
              </div>
            {/if}
          </div>
        </section>

        <section class="rounded-[18px] border border-border/80 bg-surface-2 p-5">
          {#if status?.platform === 'windows'}
            <div class="flex items-center gap-3">
              <Wrench class="text-accent" size={22} />
              <div>
                <h3 class="text-lg font-semibold text-foreground">Windows setup</h3>
                <p class="text-sm text-muted">Equalizer APO must be installed, configured for the playback device, and pointed at the managed config folder.</p>
              </div>
            </div>

            <div class="mt-5 grid gap-3 text-sm">
              <div class="rounded-[14px] border border-border/70 bg-surface p-3">
                <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Equalizer APO ConfigPath</p>
                <p class="mt-1 truncate font-mono text-xs text-foreground">
                  {status.installedConfigPath ?? 'Not detected'}
                </p>
              </div>
              <div class="rounded-[14px] border border-border/70 bg-surface p-3">
                <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Expected managed folder</p>
                <p class="mt-1 truncate font-mono text-xs text-foreground">
                  {status.configPath ?? 'Unknown'}
                </p>
              </div>
            </div>

            <div class="mt-5 flex flex-wrap gap-3">
              <Button onclick={onOpenTroubleshoot} disabled={!windowsToolsAvailable || busy}>
                Open APO setup
              </Button>
              <Button variant="secondary" onclick={onRefresh} disabled={busy}>
                {#if busy}<Loader2 class="animate-spin" size={14} />{/if}
                Refresh status
              </Button>
            </div>
          {:else if status?.platform === 'linux'}
            <div class="flex items-center gap-3">
              <CircleDot class="text-accent" size={22} />
              <div>
                <h3 class="text-lg font-semibold text-foreground">Linux setup</h3>
                <p class="text-sm text-muted">
                  SmartEQPresetSwitcher exports the active preset for Linux EQ tools. PipeWire system-wide routing still needs a running PipeWire session and may need restart/routing after first setup.
                </p>
              </div>
            </div>

            
            {#if status.detectedBackendLabel || status.detectedBackendDetail}
              <div class="mt-5 rounded-[14px] border border-border/70 bg-surface p-3 text-sm">
                <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Detected backend</p>
                <p class="mt-1 font-semibold text-foreground">{status.detectedBackendLabel ?? 'Unknown'}</p>
                {#if status.detectedBackendDetail}
                  <p class="mt-1 text-muted">{status.detectedBackendDetail}</p>
                {/if}
              </div>
            {/if}

            <div class="mt-5 grid gap-3 text-sm">
              {#if status.activeExportPath}
                <div class="rounded-[14px] border border-border/70 bg-surface p-3">
                  <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Active export</p>
                  <button
                    class="mt-1 max-w-full truncate font-mono text-xs text-foreground underline-offset-4 hover:underline"
                    type="button"
                    onclick={() => onOpenPath?.(status?.activeExportPath ?? '')}
                    title="Open export folder"
                  >
                    {status.activeExportPath}
                  </button>
                </div>
              {/if}

              {#if status.pipewireConfigPath}
                <div class="rounded-[14px] border border-border/70 bg-surface p-3">
                  <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">PipeWire setup file</p>
                  <button
                    class="mt-1 max-w-full truncate font-mono text-xs text-foreground underline-offset-4 hover:underline"
                    type="button"
                    onclick={() => onOpenPath?.(status?.pipewireConfigPath ?? '')}
                    title="Open PipeWire config folder"
                  >
                    {status.pipewireConfigPath}
                  </button>
                </div>
              {/if}
            </div>

            <div class="mt-5 rounded-[14px] border border-border/70 bg-surface p-4 text-sm leading-6 text-muted">
              Recommended Linux backends:
              <span class="font-semibold text-foreground">PipeWire filter-chain</span> for system-wide setup,
              or <span class="font-semibold text-foreground">EasyEffects</span> when the user wants a GUI EQ app. GraphicEQ-only presets are converted to a parametric approximation for PipeWire.
              {#if status?.setupHint}
                <p class="mt-3 text-amber-200">{status.setupHint}</p>
              {/if}
            </div>

            {#if status?.installCommand || status?.restartCommand}
              <div class="mt-4 grid gap-3 text-sm">
                {#if status.installCommand}
                  <div class="rounded-[14px] border border-border/70 bg-surface p-3">
                    <div class="mb-2 flex items-center justify-between gap-3">
                      <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">Distro install command</p>
                      <button class="text-xs text-accent hover:underline" type="button" onclick={() => copyText(status?.installCommand)}>Copy</button>
                    </div>
                    <p class="overflow-x-auto whitespace-nowrap font-mono text-xs text-foreground">{status.installCommand}</p>
                  </div>
                {/if}

                {#if status.restartCommand}
                  <div class="rounded-[14px] border border-border/70 bg-surface p-3">
                    <div class="mb-2 flex items-center justify-between gap-3">
                      <p class="text-[10px] font-semibold uppercase tracking-[0.16em] text-muted">User service restart</p>
                      <button class="text-xs text-accent hover:underline" type="button" onclick={() => copyText(status?.restartCommand)}>Copy</button>
                    </div>
                    <p class="overflow-x-auto whitespace-nowrap font-mono text-xs text-foreground">{status.restartCommand}</p>
                  </div>
                {/if}
              </div>
            {/if}

            <div class="mt-5 flex flex-wrap gap-3">
              <Button onclick={onSetup} disabled={busy || status?.state === 'no_active_preset'}>
                {#if busy}<Loader2 class="animate-spin" size={14} />{/if}
                {status?.setupActionLabel ?? 'Setup Linux EQ export'}
              </Button>
              <Button variant="secondary" onclick={onRefresh} disabled={busy}>
                Refresh status
              </Button>
            </div>
          {:else}
            <p class="text-sm leading-6 text-muted">
              This platform does not have a supported system EQ backend yet. Preset management still works locally.
            </p>
            <div class="mt-5">
              <Button variant="secondary" onclick={onRefresh} disabled={busy}>Refresh status</Button>
            </div>
          {/if}
        </section>
      </div>
    </section>
  </div>
{/if}
