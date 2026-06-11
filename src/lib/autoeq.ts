import { extractConvolutionPath } from '$lib/convolution';
import type { AutoEqPresetAnalysis, AutoEqPresetKind } from '$lib/types';

const GRAPHIC_EQ_RE = /^\s*GraphicEQ\s*:/im;
const FILTER_RE = /^\s*Filter\s+\d+\s*:/gim;
const PREAMP_RE = /^\s*Preamp\s*:/im;
const INCLUDE_RE = /^\s*Include\s*:/im;
const DEVICE_RE = /^\s*Device\s*:/im;
const CHANNEL_RE = /^\s*Channel\s*:/im;

function resolveKind(analysis: Omit<AutoEqPresetAnalysis, 'kind'>): AutoEqPresetKind {
  if (analysis.hasGraphicEq) {
    return 'GraphicEQ';
  }

  if (analysis.filterCount > 0) {
    return 'Filters';
  }

  if (analysis.hasConvolution) {
    return 'Convolution';
  }

  if (analysis.hasInclude || analysis.hasDevice || analysis.hasChannel) {
    return 'Config';
  }

  return 'Unknown';
}

export function analyzeAutoEqPreset(content: string): AutoEqPresetAnalysis {
  const filterCount = content.match(FILTER_RE)?.length ?? 0;
  const analysisBase = {
    filterCount,
    hasPreamp: PREAMP_RE.test(content),
    hasInclude: INCLUDE_RE.test(content),
    hasDevice: DEVICE_RE.test(content),
    hasChannel: CHANNEL_RE.test(content),
    hasConvolution: extractConvolutionPath(content) !== null,
    hasGraphicEq: GRAPHIC_EQ_RE.test(content)
  };

  return {
    ...analysisBase,
    kind: resolveKind(analysisBase)
  };
}
