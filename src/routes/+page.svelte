<script lang="ts">
  import { onMount } from 'svelte';
  import {
    AudioLines,
    CircleDot,
    Download,
    FolderInput,
    Search,
  } from '@lucide/svelte';
  import { open, save, ask } from '@tauri-apps/plugin-dialog';

  import Button from '$lib/components/ui/button.svelte';
  import Input from '$lib/components/ui/input.svelte';
  import AboutModal from '$lib/components/AboutModal.svelte';
  import AutoEqModal from '$lib/components/AutoEqModal.svelte';
  import ConfigEditorModal from '$lib/components/ConfigEditorModal.svelte';
  import EqBackendModal from '$lib/components/EqBackendModal.svelte';
  import GroupSidebar from '$lib/components/GroupSidebar.svelte';
  import LogsModal from '$lib/components/LogsModal.svelte';
  import FooterBar from '$lib/components/layout/FooterBar.svelte';
  import PresetWorkspace from '$lib/components/PresetWorkspace.svelte';
  import EditorPane from '$lib/components/EditorPane.svelte';
  import TroubleshootModal from '$lib/components/TroubleshootModal.svelte';
  import { presetStore } from '$lib/store';
  import {
    disableEq,
    exportLinuxEqStatus,
    getAutorunEnabled,
    getEqBackendStatus,
    loadAutoEqIndex,
    loadLogs,
    installOrReinstallApo,
    onAutoEqProgress,
    onOpenAboutRequested,
    onRuntimeSettingsUpdated,
    openApoDeviceSelector,
    openRepositoryUrl,
    openLogsLocation,
    setAutorunEnabled,
    setupLinuxSystemEq
  } from '$lib/tauri';
  import type {
    AppRuntimeSettings,
    AutoEqIndexEntry,
    AutoEqProgressPayload,
    EqBackendStatus,
    LogSnapshot,
    PresetGroup,
    PresetItem,
    PresetLibrary
  } from '$lib/types';
  import { sanitizeImportName, uniqueName } from '$lib/utils';

  let library: PresetLibrary | null = null;
  let selectedGroupName: string | null = null;
  let selectedPresetName: string | null = null;
  let draft = '';
  let dirty = false;
  let busy = false;
  let search = '';
  let configEditorOpen = false;
  let aboutOpen = false;
  let troubleshootOpen = false;
  let logsOpen = false;
  let logsLoading = false;
  let logsContent = '';
  let logsPath = '';
  let logsExists = false;
  let statusMessage = 'Loading presets...';
  let statusTone: 'info' | 'success' | 'error' = 'info';
  let autoEqOpen = false;
  let autoEqWarmupState: 'idle' | 'loading' | 'ready' | 'stale' | 'error' = 'idle';
  let autoEqWarmupMessage = '';
  let autorunEnabled = false;
  let autorunLoaded = false;
  let autorunBusy = false;
  let windowsToolsAvailable = false;
  let eqBackendOpen = false;
  let eqBackendStatus: EqBackendStatus | null = null;
  let eqBackendBusy = false;

  onMount(() => {
    windowsToolsAvailable = navigator.userAgent.toLowerCase().includes('windows');
    const unsubscribe = presetStore.subscribe((value) => {
      const preserveDraft = dirty && selectionStillExists(value);
      syncSelection(value, preserveDraft);
    });
    let disposed = false;
    let unlistenRuntimeSettings: (() => void) | null = null;
    let unlistenOpenAbout: (() => void) | null = null;
    let unlistenAutoEqProgress: (() => void) | null = null;

    void onRuntimeSettingsUpdated((value) => {
      if (!disposed) {
        syncRuntimeSettings(value);
      }
    })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlistenRuntimeSettings = unlisten;
      })
      .catch((error) => setStatus(getErrorMessage(error), 'error'));

    void onOpenAboutRequested(() => {
      if (!disposed) {
        handleOpenAbout();
      }
    })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlistenOpenAbout = unlisten;
      })
      .catch((error) => setStatus(getErrorMessage(error), 'error'));

    void onAutoEqProgress((value) => {
      if (!disposed) {
        syncAutoEqWarmup(value);
      }
    })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlistenAutoEqProgress = unlisten;
      })
      .catch((error) => {
        autoEqWarmupState = 'error';
        autoEqWarmupMessage = getErrorMessage(error);
      });

    void loadAutorunState();

    void presetStore
      .start()
      .then(() => {
        setStatus('Ready to manage EQ presets.');
        void refreshEqBackendStatus(false);
        setTimeout(() => {
          void prefetchAutoEqIndex();
        }, 0);
      })
      .catch((error) => setStatus(getErrorMessage(error), 'error'));

    return () => {
      disposed = true;
      unsubscribe();
      unlistenRuntimeSettings?.();
      unlistenOpenAbout?.();
      unlistenAutoEqProgress?.();
      void presetStore.stop();
    };
  });

  function selectionStillExists(next: PresetLibrary | null) {
    if (!next || !selectedGroupName || !selectedPresetName) {
      return false;
    }

    return next.groups.some(
      (group) =>
        group.name === selectedGroupName &&
        group.presets.some((preset) => preset.name === selectedPresetName)
    );
  }

  function syncSelection(next: PresetLibrary | null, preserveDraft = false) {
    library = next;
    if (!next || next.groups.length === 0) {
      selectedGroupName = null;
      selectedPresetName = null;
      if (!preserveDraft) {
        draft = '';
        dirty = false;
      }
      return;
    }

    if (!selectedGroupName || !next.groups.some((group) => group.name === selectedGroupName)) {
      selectedGroupName = next.groups[0]?.name ?? null;
    }

    const group = currentGroup(next);
    if (!group) {
      selectedPresetName = null;
      if (!preserveDraft) {
        draft = '';
        dirty = false;
      }
      return;
    }

    if (!selectedPresetName || !group.presets.some((preset) => preset.name === selectedPresetName)) {
      selectedPresetName = group.presets[0]?.name ?? null;
    }

    if (!preserveDraft) {
      draft = currentPreset(next)?.content ?? '';
      dirty = false;
    }
  }

  function currentGroup(snapshot: PresetLibrary | null = library): PresetGroup | null {
    return snapshot?.groups.find((group) => group.name === selectedGroupName) ?? null;
  }

  function currentPreset(snapshot: PresetLibrary | null = library): PresetItem | null {
    return (
      currentGroup(snapshot)?.presets.find((preset) => preset.name === selectedPresetName) ?? null
    );
  }

  function presetForGroup(snapshot: PresetLibrary | null, groupName: string, presetName: string) {
    return (
      snapshot?.groups.find((group) => group.name === groupName)?.presets.find((preset) => preset.name === presetName) ??
      null
    );
  }

  function isSelectedPreset(groupName: string, presetName: string) {
    return selectedGroupName === groupName && selectedPresetName === presetName;
  }

  async function confirmDiscardIfNeeded() {
    if (!dirty) {
      return true;
    }

    return ask('Discard the current unsaved preset edits?', {
      title: 'Unsaved changes',
      kind: 'warning'
    });
  }

  function setStatus(message: string, tone: 'info' | 'success' | 'error' = 'info') {
    statusMessage = message;
    statusTone = tone;
  }

  function syncRuntimeSettings(value: AppRuntimeSettings) {
    autorunEnabled = value.autorunEnabled;
    autorunLoaded = true;
    autorunBusy = false;
  }

  function syncAutoEqWarmup(value: AutoEqProgressPayload) {
    if (value.operation !== 'index') {
      return;
    }

    autoEqWarmupMessage = value.message;

    if (value.phase === 'error') {
      autoEqWarmupState = 'error';
      return;
    }

    if (
      value.phase === 'start' ||
      value.phase === 'check-cache' ||
      value.phase === 'fetch-index'
    ) {
      autoEqWarmupState = 'loading';
      return;
    }

    if (value.source === 'stale-cache') {
      autoEqWarmupState = 'stale';
      return;
    }

    if (value.phase === 'done' || value.phase === 'cache-hit') {
      autoEqWarmupState = 'ready';
    }
  }

  function getErrorMessage(error: unknown) {
    if (typeof error === 'string') {
      return error;
    }
    if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
      return error.message;
    }
    return 'An unexpected error occurred.';
  }

  async function withBusy<T>(task: () => Promise<T>, successMessage?: string) {
    busy = true;
    try {
      const result = await task();
      if (successMessage) {
        setStatus(successMessage, 'success');
      }
      return result;
    } catch (error) {
      setStatus(getErrorMessage(error), 'error');
      return null;
    } finally {
      busy = false;
    }
  }

  async function loadAutorunState() {
    try {
      syncRuntimeSettings({
        autorunEnabled: await getAutorunEnabled()
      });
    } catch (error) {
      autorunLoaded = true;
      setStatus(getErrorMessage(error), 'error');
    }
  }

  async function prefetchAutoEqIndex() {
    if (autoEqWarmupState === 'loading' || autoEqWarmupState === 'ready') {
      return;
    }

    autoEqWarmupState = 'loading';
    autoEqWarmupMessage = 'Preparing AutoEQ index.';

    try {
      await loadAutoEqIndex(false);
    } catch (error) {
      autoEqWarmupState = 'error';
      autoEqWarmupMessage = getErrorMessage(error);
    }
  }

  async function handleGroupSelect(groupName: string) {
    if (!(await confirmDiscardIfNeeded())) {
      return;
    }

    selectedGroupName = groupName;
    selectedPresetName = currentGroup()?.presets[0]?.name ?? null;
    draft = currentPreset()?.content ?? '';
    dirty = false;
  }

  async function handlePresetSelect(presetName: string) {
    if (!(await confirmDiscardIfNeeded())) {
      return;
    }

    selectedPresetName = presetName;
    draft = currentPreset()?.content ?? '';
    dirty = false;
  }

  async function handleCreateGroup(value: { name: string; emoji: string | null }) {
    const { name, emoji } = value;
    const snapshot = await withBusy(() => presetStore.createGroup(name), `Created group ${name}`);
    if (snapshot) {
      selectedGroupName = name;
      selectedPresetName = null;
      draft = '';
      dirty = false;

      if (emoji) {
        await withBusy(
          () => presetStore.setGroupEmoji(name, emoji),
          `Set emoji for ${name}`
        );
      }
    }
  }

  async function handleRenameGroup(value: { oldName: string; newName: string }) {
    const { oldName, newName } = value;
    const snapshot = await withBusy(
      () => presetStore.renameGroup(oldName, newName),
      `Renamed ${oldName} to ${newName}`
    );
    if (snapshot && selectedGroupName === oldName) {
      selectedGroupName = newName;
    }
  }

  async function handleDeleteGroup(groupName: string) {
    const confirmed = await ask(`Delete the group "${groupName}" and all presets inside it?`, {
      title: 'Delete group',
      kind: 'warning'
    });
    if (!confirmed) {
      return;
    }

    await withBusy(() => presetStore.deleteGroup(groupName), `Deleted group ${groupName}`);
  }

  async function handleSetGroupEmoji(value: { groupName: string; emoji: string | null }) {
    await withBusy(
      () => presetStore.setGroupEmoji(value.groupName, value.emoji),
      value.emoji ? `Updated emoji for ${value.groupName}` : `Cleared emoji for ${value.groupName}`
    );
  }


  async function handleCreatePreset(presetName: string) {
    if (!selectedGroupName) {
      return;
    }

    const snapshot = await withBusy(
      () => presetStore.createPreset(selectedGroupName as string, presetName, ''),
      `Created preset ${presetName}`
    );
    if (snapshot) {
      selectedPresetName = presetName;
      draft = '';
      dirty = false;
    }
  }

  async function handleRenamePreset(value: { oldName: string; newName: string }) {
    const { oldName, newName } = value;
    if (!selectedGroupName) {
      return;
    }

    const snapshot = await withBusy(
      () => presetStore.renamePreset(selectedGroupName as string, oldName, newName),
      `Renamed ${oldName} to ${newName}`
    );
    if (snapshot && selectedPresetName === oldName) {
      selectedPresetName = newName;
    }
  }

  async function handleDeletePreset(presetName: string) {
    if (!selectedGroupName) {
      return;
    }

    const confirmed = await ask(`Delete the preset "${presetName}"?`, {
      title: 'Delete preset',
      kind: 'warning'
    });
    if (!confirmed) {
      return;
    }

    await withBusy(
      () => presetStore.deletePreset(selectedGroupName as string, presetName),
      `Deleted preset ${presetName}`
    );
  }

  async function handleMovePreset(event: { oldGroup: string; newGroup: string; name: string; targetIndex?: number }) {
    const { oldGroup, newGroup, name, targetIndex } = event;
    const snapshot = await withBusy(
      () => presetStore.movePreset(oldGroup, newGroup, name, targetIndex),
      oldGroup === newGroup ? `Reordered ${name}` : `Moved ${name} to ${newGroup}`
    );
    if (snapshot && selectedPresetName === name) {
      selectedGroupName = newGroup;
    }
  }

  async function handleSave() {
    if (!selectedGroupName || !selectedPresetName) {
      return;
    }

    const snapshot = await withBusy(
      () => presetStore.savePreset(selectedGroupName as string, selectedPresetName as string, draft),
      `Saved ${selectedPresetName}`
    );
    if (snapshot) {
      dirty = false;
      await refreshEqBackendStatus(false);
    }
  }

  async function handleApply() {
    if (!selectedGroupName || !selectedPresetName) {
      return;
    }

    if (dirty) {
      const saved = await withBusy(
        () => presetStore.savePreset(selectedGroupName as string, selectedPresetName as string, draft),
        `Saved ${selectedPresetName}`
      );
      if (!saved) {
        return;
      }
      dirty = false;
    }

    const snapshot = await withBusy(
      () => presetStore.applyPreset(selectedGroupName as string, selectedPresetName as string),
      `Applied ${selectedPresetName}`
    );

    if (snapshot) {
      await refreshEqBackendStatus(false);
    }
  }

  async function handleApplyPreset(name: string) {
    if (selectedPresetName !== name) {
      selectedPresetName = name;
      draft = currentPreset()?.content ?? '';
      dirty = false;
    }

    await handleApply();
  }

  async function handleImportPresets() {
    const selection = await open({
      multiple: true,
      filters: [{ name: 'EQ preset files or WAV files', extensions: ['txt', 'wav'] }]
    });

    const paths = Array.isArray(selection) ? selection : selection ? [selection] : [];
    if (paths.length === 0) {
      return;
    }

    let targetGroupName = selectedGroupName;
    if (!targetGroupName) {
      const nextGroupName = uniqueName(
        'Imported',
        library?.groups.map((group) => group.name) ?? []
      );
      const snapshot = await withBusy(() => presetStore.createGroup(nextGroupName), `Created group ${nextGroupName}`);
      if (!snapshot) {
        return;
      }
      targetGroupName = nextGroupName;
      selectedGroupName = nextGroupName;
    }

    const snapshot = await withBusy(
      () => presetStore.importPresets(targetGroupName as string, paths),
      `Imported ${paths.length} preset${paths.length === 1 ? '' : 's'}`
    );

    if (snapshot) {
      await refreshEqBackendStatus(false);
    }
  }

  function handleOpenAutoEq() {
    autoEqOpen = true;
  }

  function handleCloseAutoEq() {
    autoEqOpen = false;
  }

  async function handleImportAutoEq(value: {
    entry: AutoEqIndexEntry;
    presetText: string;
  }) {
    if (!(await confirmDiscardIfNeeded())) {
      return false;
    }

    let targetGroupName = selectedGroupName;
    let snapshotContext = library;

    if (!targetGroupName) {
      const nextGroupName = uniqueName(
        'Imported',
        snapshotContext?.groups.map((group) => group.name) ?? []
      );
      const createdGroup = await withBusy(
        () => presetStore.createGroup(nextGroupName),
        `Created group ${nextGroupName}`
      );
      if (!createdGroup) {
        return false;
      }

      targetGroupName = nextGroupName;
      snapshotContext = createdGroup;
    }

    const existingPresetNames =
      snapshotContext?.groups
        .find((group) => group.name === targetGroupName)
        ?.presets.map((preset) => preset.name) ?? [];

    const presetName = uniqueName(
      sanitizeImportName(`${value.entry.n} (${value.entry.s}) GraphicEQ`),
      existingPresetNames
    );

    const snapshot = await withBusy(
      () => presetStore.createPreset(targetGroupName as string, presetName, value.presetText),
      `Imported ${presetName} from AutoEQ`
    );
    if (!snapshot) {
      return false;
    }

    selectedGroupName = targetGroupName;
    selectedPresetName = presetName;
    draft = presetForGroup(snapshot, targetGroupName as string, presetName)?.content ?? value.presetText;
    dirty = false;
    return true;
  }

  async function handleToggleConvolution(value: { groupName: string; presetName: string; enabled: boolean }) {
    const preset = presetForGroup(library, value.groupName, value.presetName);
    const baseContent =
      isSelectedPreset(value.groupName, value.presetName) && dirty
        ? draft
        : preset?.content ?? '';

    if (value.enabled) {
      const selection = await open({
        multiple: false,
        filters: [{ name: 'Convolution WAV', extensions: ['wav'] }]
      });

      if (typeof selection !== 'string') {
        return false;
      }

      const snapshot = await withBusy(
        () =>
          presetStore.attachConvolutionWav(
            value.groupName,
            value.presetName,
            baseContent,
            selection
          ),
        `Linked convolution WAV for ${value.presetName}`
      );

      if (!snapshot) {
        return false;
      }

      if (isSelectedPreset(value.groupName, value.presetName)) {
        draft = presetForGroup(snapshot, value.groupName, value.presetName)?.content ?? draft;
        dirty = false;
      }

      return true;
    }

    const snapshot = await withBusy(
      () => presetStore.removeConvolutionWav(value.groupName, value.presetName, baseContent),
      `Removed convolution WAV from ${value.presetName}`
    );

    if (!snapshot) {
      return false;
    }

    if (isSelectedPreset(value.groupName, value.presetName)) {
      draft = presetForGroup(snapshot, value.groupName, value.presetName)?.content ?? draft;
      dirty = false;
    }

    return true;
  }

  async function handleImportAppData() {
    const selection = await open({
      multiple: false,
      filters: [{ name: 'SmartEQPresetSwitcher Backup', extensions: ['json'] }]
    });

    if (typeof selection !== 'string') {
      return;
    }

    const confirmed = await ask(
      'Import app data from this backup file? This will replace the current groups, presets, and settings.',
      {
        title: 'Import App Data',
        kind: 'warning'
      }
    );

    if (!confirmed) {
      return;
    }

    await withBusy(() => presetStore.importAppSettings(selection), 'Imported app settings');
  }

  async function handleInstallOrReinstallApo() {
    await withBusy(
      () => installOrReinstallApo(),
      'Equalizer APO finished installing and Device Selector opened.'
    );
  }

  async function handleOpenApoDeviceSelector() {
    await withBusy(() => openApoDeviceSelector(), 'Device Selector opened.');
  }

  async function loadLogsSnapshot() {
    logsLoading = true;
    try {
      const snapshot: LogSnapshot = await loadLogs();
      logsContent = snapshot.content;
      logsPath = snapshot.logPath;
      logsExists = snapshot.exists;
    } catch (error) {
      logsContent = getErrorMessage(error);
      logsPath = '';
      logsExists = false;
      setStatus(getErrorMessage(error), 'error');
    } finally {
      logsLoading = false;
    }
  }

  function handleOpenLogs() {
    logsOpen = true;
    void loadLogsSnapshot();
  }

  function handleCloseLogs() {
    logsOpen = false;
  }

  async function handleOpenLogsLocation() {
    try {
      await openLogsLocation();
    } catch (error) {
      setStatus(getErrorMessage(error), 'error');
    }
  }

  async function handleOpenRepository() {
    try {
      await openRepositoryUrl();
    } catch (error) {
      setStatus(getErrorMessage(error), 'error');
    }
  }

  async function handleExport() {
    if (!selectedGroupName || !selectedPresetName) {
      return;
    }

    const destination = await save({
      defaultPath: `${selectedPresetName}.txt`,
      filters: [{ name: 'EQ Presets', extensions: ['txt'] }]
    });

    if (!destination) {
      return;
    }

    await withBusy(
      () => presetStore.exportPreset(selectedGroupName as string, selectedPresetName as string, destination),
      `Exported ${selectedPresetName}`
    );
  }

  async function handleExportAppSettings() {
    const destination = await save({
      defaultPath: 'smart-eq-preset-switcher-backup.json',
      filters: [{ name: 'JSON', extensions: ['json'] }]
    });

    if (!destination) {
      return;
    }

    await withBusy(() => presetStore.exportAppSettings(destination), 'Exported app settings');
  }

  async function handleAutorunToggle(event: Event) {
    const nextEnabled = (event.currentTarget as HTMLInputElement).checked;
    autorunBusy = true;

    try {
      const actualEnabled = await setAutorunEnabled(nextEnabled);
      syncRuntimeSettings({
        autorunEnabled: actualEnabled
      });
      setStatus(
        actualEnabled
          ? 'Start with login enabled.'
          : 'Start with login disabled.',
        'success'
      );
    } catch (error) {
      autorunBusy = false;
      setStatus(getErrorMessage(error), 'error');
    }
  }



  function eqStatusTone(status: EqBackendStatus | null) {
    if (!status) {
      return 'bg-accent-soft text-accent border-accent/20';
    }

    if (status.state === 'eq_disabled') {
      return 'bg-amber-500/10 text-amber-200 border-amber-400/25';
    }

    if (status.state === 'connected' || status.state === 'export_ready') {
      return 'bg-success-soft text-success border-success/25';
    }

    if (status.state === 'setup_needed' || status.state === 'no_active_preset') {
      return 'bg-amber-500/10 text-amber-200 border-amber-400/25';
    }

    return 'bg-surface-2 text-muted border-border';
  }

  function eqStatusDot(status: EqBackendStatus | null) {
    if (!status) {
      return 'bg-accent';
    }

    if (status.state === 'eq_disabled') {
      return 'bg-amber-300';
    }

    if (status.state === 'connected' || status.state === 'export_ready') {
      return 'bg-success';
    }

    if (status.state === 'setup_needed' || status.state === 'no_active_preset') {
      return 'bg-amber-300';
    }

    return 'bg-muted';
  }

  async function refreshEqBackendStatus(showError = true) {
    try {
      eqBackendStatus = await getEqBackendStatus();
    } catch (error) {
      if (showError) {
        setStatus(getErrorMessage(error), 'error');
      }
    }
  }

  function handleOpenEqBackend() {
    eqBackendOpen = true;
    void refreshEqBackendStatus();
  }

  function handleCloseEqBackend() {
    eqBackendOpen = false;
  }

  async function handleSetupEqBackend() {
    eqBackendBusy = true;

    try {
      if (eqBackendStatus?.platform === 'linux') {
        eqBackendStatus = await setupLinuxSystemEq();
        setStatus(eqBackendStatus.statusLabel, 'success');
      } else if (eqBackendStatus?.platform === 'windows') {
        handleOpenTroubleshoot();
      } else {
        await refreshEqBackendStatus();
      }
    } catch (error) {
      setStatus(getErrorMessage(error), 'error');
    } finally {
      eqBackendBusy = false;
    }
  }

  async function handleDisableEq() {
    const status = await withBusy(() => disableEq(), 'EQ bypassed');
    if (status) {
      await refreshEqBackendStatus(false);
    }
  }

  function handleOpenEqBackendPath(path: string) {
    if (!path) {
      return;
    }

    void openBackendPath(path);
  }

  async function openBackendPath(path: string) {
    try {
      const { revealPathInExplorer } = await import('$lib/tauri');
      await revealPathInExplorer(path);
    } catch (error) {
      setStatus(getErrorMessage(error), 'error');
    }
  }

  function handleOpenConfigEditor() {
    if (selectedGroupName && selectedPresetName) {
      configEditorOpen = true;
    }
  }

  function handleCloseConfigEditor() {
    configEditorOpen = false;
  }

  function handleOpenAbout() {
    aboutOpen = true;
  }

  function handleCloseAbout() {
    aboutOpen = false;
  }

  function handleOpenTroubleshoot() {
    if (!windowsToolsAvailable) {
      setStatus('Equalizer APO tools are Windows-only. Linux uses config export/TUI workflows.', 'info');
      return;
    }
    troubleshootOpen = true;
  }

  function handleCloseTroubleshoot() {
    troubleshootOpen = false;
  }
</script>

<svelte:head>
  <title>SmartEQPresetSwitcher</title>
  <meta
    name="description"
    content="Cross-platform EQ preset switching for Windows and Linux."
  />
</svelte:head>

<div class="min-h-screen bg-background text-foreground">
  <div class="mx-auto flex min-h-screen max-w-[1920px] flex-col px-4 py-4 sm:px-5">
    <header class="shell-surface mb-4 overflow-hidden p-4 shadow-[0_12px_30px_rgba(0,0,0,0.25)]">
      <div class="grid gap-4 xl:grid-cols-[1.2fr_1fr_auto] xl:items-center">
        <div class="flex items-center gap-4">
          <div class="flex size-11 items-center justify-center rounded-[12px] border border-accent/30 bg-accent-soft text-accent">
            <AudioLines size={22} />
          </div>
          <div class="min-w-0">
            <p class="text-[11px] font-semibold uppercase tracking-[0.22em] text-muted">
              SmartEQPresetSwitcher
            </p>
            <h1 class="mt-1 text-[20px] font-semibold tracking-tight text-foreground sm:text-[22px]">
              Cross-platform EQ preset switcher
            </h1>
            <p class="mt-1 max-w-3xl text-sm leading-6 text-muted">
              One active preset per group. Apply changes instantly, keep the tray checkmarks in sync, and work from a writable app folder.
            </p>
          </div>
        </div>

        <div class="shell-surface-2 flex items-center gap-3 px-3 py-2">
          <Search size={17} class="shrink-0 text-muted" />
          <Input
            bind:value={search}
            placeholder="Search groups, presets, or config text"
            class="border-0 bg-transparent px-0 shadow-none focus-visible:ring-0"
          />
        </div>

        <div class="flex flex-wrap justify-start gap-2 xl:justify-end">
          <Button
            variant="secondary"
            onclick={handleOpenAutoEq}
            disabled={busy}
            title={autoEqWarmupMessage || 'Import from AutoEQ'}
          >
            <span class="text-[15px] leading-none text-accent">🎧</span>
            Import from AutoEQ
            {#if autoEqWarmupState !== 'idle'}
              <span
                class={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.08em] ${
                  autoEqWarmupState === 'ready'
                    ? 'bg-success-soft text-success'
                    : autoEqWarmupState === 'stale'
                      ? 'bg-amber-500/12 text-amber-300'
                      : autoEqWarmupState === 'error'
                        ? 'bg-danger-soft text-danger'
                        : 'bg-accent-soft text-accent'
                }`}
              >
                <span
                  class={`h-1.5 w-1.5 rounded-full ${
                    autoEqWarmupState === 'ready'
                      ? 'bg-success'
                      : autoEqWarmupState === 'stale'
                        ? 'bg-amber-300'
                        : autoEqWarmupState === 'error'
                          ? 'bg-danger'
                          : 'animate-pulse bg-accent'
                  }`}
                ></span>
                {#if autoEqWarmupState === 'ready'}
                  Warm
                {:else if autoEqWarmupState === 'stale'}
                  Cached
                {:else if autoEqWarmupState === 'error'}
                  Retry
                {:else}
                  Warming
                {/if}
              </span>
            {/if}
          </Button>
          <Button
            variant="secondary"
            onclick={handleOpenEqBackend}
            disabled={busy}
            title={eqBackendStatus?.statusDetail ?? 'Check EQ backend connection'}
            class={`border ${eqStatusTone(eqBackendStatus)}`}
          >
            <CircleDot size={14} class={eqStatusDot(eqBackendStatus)} />
            {eqBackendStatus?.statusLabel ?? 'EQ backend'}
          </Button>
          {#if eqBackendStatus?.state === 'export_ready'}
            <Button
              variant="secondary"
              onclick={handleDisableEq}
              disabled={busy}
              class="border border-amber-400/25 bg-amber-500/10 text-amber-200"
            >
              Bypass EQ
            </Button>
          {/if}
          {#if eqBackendStatus?.state === 'eq_disabled'}
            <span class="text-xs text-amber-400">
              EQ bypassed
            </span>
          {/if}
          <Button variant="secondary" onclick={handleImportAppData}>
            <FolderInput size={14} />
            Import App Data
          </Button>
          <Button variant="secondary" onclick={handleExportAppSettings} disabled={!library}>
            <Download size={14} />
            Export App Data
          </Button>
        </div>
      </div>

    </header>

    <main class="grid min-h-0 flex-1 gap-4 overflow-y-auto xl:grid-cols-[300px_minmax(340px,1fr)_460px]">
      <GroupSidebar
        groups={library?.groups ?? []}
        {selectedGroupName}
        {search}
        onSelect={handleGroupSelect}
        onCreate={handleCreateGroup}
        onRename={handleRenameGroup}
        onDelete={handleDeleteGroup}
        onMovePreset={handleMovePreset}
        onEmojiChange={handleSetGroupEmoji}
      />

      <PresetWorkspace
        group={currentGroup()}
        {selectedPresetName}
        {search}
        presetFilePath={
          library && selectedGroupName && selectedPresetName
            ? `${library.appDataDir}/presets/${selectedGroupName}/${selectedPresetName}.txt`
            : null
        }
        onSelect={handlePresetSelect}
        onCreate={handleCreatePreset}
        onRename={handleRenamePreset}
        onDelete={handleDeletePreset}
        onApply={handleApplyPreset}
        onMove={handleMovePreset}
        onImport={handleImportPresets}
      />

      <EditorPane
        groupName={selectedGroupName}
        presetName={selectedPresetName}
        configPath={library?.configPath ?? null}
        panelKey={selectedGroupName && selectedPresetName ? `${selectedGroupName}::${selectedPresetName}` : ''}
        presetConvolution={currentPreset()?.convolution ?? null}
        {draft}
        {dirty}
        onSave={handleSave}
        onApply={handleApply}
        onExport={handleExport}
        onEditConfig={handleOpenConfigEditor}
        onToggleConvolution={handleToggleConvolution}
      />
    </main>

    <ConfigEditorModal
      open={configEditorOpen}
      groupName={selectedGroupName}
      presetName={selectedPresetName}
      {draft}
      {dirty}
      presetFilePath={
        library && selectedGroupName && selectedPresetName
          ? `${library.appDataDir}/presets/${selectedGroupName}/${selectedPresetName}.txt`
          : null
      }
      configPath={library?.configPath ?? null}
      configTargetLabel="Backend config"
      panelKey={selectedGroupName && selectedPresetName ? `${selectedGroupName}::${selectedPresetName}` : ''}
      presetConvolution={currentPreset()?.convolution ?? null}
      onDraftChange={(value) => { draft = value; dirty = true; }}
      onSave={handleSave}
      onClose={handleCloseConfigEditor}
      onToggleConvolution={handleToggleConvolution}
    />

    <AboutModal
      open={aboutOpen}
      onClose={handleCloseAbout}
      onOpenRepository={handleOpenRepository}
    />

    {#if windowsToolsAvailable}
      <TroubleshootModal
        open={troubleshootOpen}
        library={library}
        onClose={handleCloseTroubleshoot}
        onInstall={handleInstallOrReinstallApo}
        onOpenSelector={handleOpenApoDeviceSelector}
      />
    {/if}

    <LogsModal
      open={logsOpen}
      loading={logsLoading}
      exists={logsExists}
      logPath={logsPath}
      content={logsContent}
      onClose={handleCloseLogs}
      onRefresh={loadLogsSnapshot}
      onOpenLocation={handleOpenLogsLocation}
    />

    <AutoEqModal
      open={autoEqOpen}
      targetGroupName={selectedGroupName}
      eqBackendStatus={eqBackendStatus}
      onClose={handleCloseAutoEq}
      onImport={handleImportAutoEq}
    />

    <EqBackendModal
      open={eqBackendOpen}
      status={eqBackendStatus}
      busy={eqBackendBusy}
      {windowsToolsAvailable}
      onClose={handleCloseEqBackend}
      onRefresh={() => refreshEqBackendStatus()}
      onSetup={handleSetupEqBackend}
      onOpenTroubleshoot={handleOpenTroubleshoot}
      onOpenPath={handleOpenEqBackendPath}
    />

    <FooterBar
      statusMessage={statusMessage}
      statusTone={statusTone}
      busy={busy}
      autorunEnabled={autorunEnabled}
      autorunLoaded={autorunLoaded}
      autorunBusy={autorunBusy}
      onOpenLogs={handleOpenLogs}
      onOpenTroubleshoot={handleOpenTroubleshoot}
      onOpenAbout={handleOpenAbout}
      onAutorunToggle={handleAutorunToggle}
      showTroubleshoot={windowsToolsAvailable}
    />
  </div>
</div>
