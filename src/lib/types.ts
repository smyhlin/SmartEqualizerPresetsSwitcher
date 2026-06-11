export type PresetConvolution = {
  wavPath: string;
  wavBase64?: string | null;
  error?: string | null;
};

export type PresetItem = {
  name: string;
  order: number;
  content: string;
  convolution?: PresetConvolution | null;
};

export type PresetGroup = {
  name: string;
  order: number;
  emoji: string | null;
  activePreset: string | null;
  presets: PresetItem[];
};

export type PresetLibrary = {
  appDataDir: string;
  configPath: string;
  defaultConfigPath: string;
  installedConfigPath: string | null;
  groups: PresetGroup[];
  needsConfigMigration: boolean;
  configPathPrompted: boolean;
};

export type AppRuntimeSettings = {
  autorunEnabled: boolean;
};

export type LogSnapshot = {
  logPath: string;
  content: string;
  exists: boolean;
};

export type AutoEqPresetVariant = 'auto' | 'parametric' | 'graphic';

export type AutoEqIndexEntry = {
  n: string;
  s: string;
  r: number;
  i: number;
};

export type AutoEqProgressOperation = 'index' | 'preset';

export type AutoEqProgressPhase =
  | 'start'
  | 'check-cache'
  | 'fetch-index'
  | 'fetch-version'
  | 'download-archive'
  | 'extract-preset'
  | 'cache-hit'
  | 'done'
  | 'error';

export type AutoEqProgressSource = 'cache' | 'network' | 'stale-cache';

export type AutoEqProgressPayload = {
  operation: AutoEqProgressOperation;
  phase: AutoEqProgressPhase;
  message: string;
  source?: AutoEqProgressSource | null;
  presetName?: string | null;
  presetSource?: string | null;
};

export type AutoEqPresetKind =
  | 'GraphicEQ'
  | 'Filters'
  | 'Convolution'
  | 'Config'
  | 'Unknown';

export type AutoEqPresetAnalysis = {
  kind: AutoEqPresetKind;
  filterCount: number;
  hasPreamp: boolean;
  hasInclude: boolean;
  hasDevice: boolean;
  hasChannel: boolean;
  hasConvolution: boolean;
  hasGraphicEq: boolean;
};


export type EqBackendPlatform = 'windows' | 'linux' | 'macos' | 'unknown';

export type EqBackendState =
  | 'connected'
  | 'export_ready'
  | 'setup_needed'
  | 'no_active_preset'
  | 'unsupported';

export type EqBackendStatus = {
  platform: EqBackendPlatform;
  state: EqBackendState;
  backendName: string;
  statusLabel: string;
  statusDetail: string;
  activeGroupName?: string | null;
  activePresetName?: string | null;
  configPath?: string | null;
  installedConfigPath?: string | null;
  activeExportPath?: string | null;
  pipewireConfigPath?: string | null;
  setupActionLabel: string;
  detectedBackendLabel?: string | null;
  detectedBackendDetail?: string | null;
  installCommand?: string | null;
  restartCommand?: string | null;
  setupHint?: string | null;
};
