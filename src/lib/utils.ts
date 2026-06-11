import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function uniqueName(baseName: string, existingNames: string[]) {
  const normalized = baseName.trim() || 'Untitled';
  if (!existingNames.includes(normalized)) {
    return normalized;
  }

  let suffix = 2;
  while (existingNames.includes(`${normalized} ${suffix}`)) {
    suffix += 1;
  }

  return `${normalized} ${suffix}`;
}

export function sanitizeImportName(name: string, fallback = 'Imported Preset') {
  const cleaned = name
    .replace(/[<>:"/\\|?*]/g, ' ')
    .trim()
    .split(/\s+/)
    .join(' ');

  return cleaned || fallback;
}
