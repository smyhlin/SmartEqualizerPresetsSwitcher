import { get, writable } from 'svelte/store';

import type { PresetLibrary } from '$lib/types';
import * as backend from '$lib/tauri';

const library = writable<PresetLibrary | null>(null);

let started = false;
let unlistenPromise: Promise<() => void> | null = null;

async function refresh() {
  const snapshot = await backend.loadPresets();
  library.set(snapshot);
  return snapshot;
}

async function runMutation(task: () => Promise<PresetLibrary>) {
  const snapshot = await task();
  library.set(snapshot);
  return snapshot;
}

export const presetStore = {
  subscribe: library.subscribe,
  start: async () => {
    if (!started) {
      started = true;
      unlistenPromise = backend.onPresetsUpdated((payload) => {
        library.set(payload);
      });
    }

    return refresh();
  },
  stop: async () => {
    if (unlistenPromise) {
      const unlisten = await unlistenPromise;
      unlisten();
      unlistenPromise = null;
      started = false;
    }
  },
  snapshot: () => get(library),
  refresh,
  setConfigPath: (newPath: string) => runMutation(() => backend.setConfigPath(newPath)),
  applyPreset: (group: string, name: string) => runMutation(() => backend.applyPreset(group, name)),
  savePreset: (group: string, name: string, content: string) =>
    runMutation(() => backend.savePreset(group, name, content)),
  createGroup: (name: string) => runMutation(() => backend.createGroup(name)),
  setGroupEmoji: (group: string, emoji: string | null) =>
    runMutation(() => backend.setGroupEmoji(group, emoji)),
  renameGroup: (oldName: string, newName: string) =>
    runMutation(() => backend.renameGroup(oldName, newName)),
  deleteGroup: (name: string) => runMutation(() => backend.deleteGroup(name)),
  reorderGroups: (order: string[]) => runMutation(() => backend.reorderGroups(order)),
  createPreset: (group: string, name: string, content = '') =>
    runMutation(() => backend.createPreset(group, name, content)),
  renamePreset: (group: string, oldName: string, newName: string) =>
    runMutation(() => backend.renamePreset(group, oldName, newName)),
  deletePreset: (group: string, name: string) =>
    runMutation(() => backend.deletePreset(group, name)),
  movePreset: (oldGroup: string, newGroup: string, name: string, targetIndex?: number) =>
    runMutation(() => backend.movePreset(oldGroup, newGroup, name, targetIndex)),
  importPresets: (group: string, paths: string[]) =>
    runMutation(() => backend.importPresets(group, paths)),
  attachConvolutionWav: (group: string, name: string, content: string, sourcePath: string) =>
    runMutation(() => backend.attachConvolutionWav(group, name, content, sourcePath)),
  removeConvolutionWav: (group: string, name: string, content: string) =>
    runMutation(() => backend.removeConvolutionWav(group, name, content)),
  exportPreset: (group: string, name: string, destination: string) =>
    backend.exportPreset(group, name, destination),
  exportAppSettings: (destination: string) => backend.exportAppSettings(destination),
  importAppSettings: (source: string) => runMutation(() => backend.importAppSettings(source)),
  rebuildTrayMenu: async () => {
    const snapshot = await backend.rebuildTrayMenu();
    library.set(snapshot);
    return snapshot;
  }
};
