import type { CategoryDraft, DropMode, PathInfo, TargetRule } from '../types';

const MAX_NAME_EXTENSIONS = 3;

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Case-insensitive path key; Windows paths are case-insensitive. */
function dirKey(dir: string): string {
  return dir.replace(/[\\/]+$/, '').toLowerCase();
}

function exactNamesRule(dir: string, names: string[]): TargetRule {
  const alternatives = names.map(escapeRegex).join('|');
  return {
    id: '',
    dir,
    matchType: 'regex',
    pattern: names.length === 1 ? `^${alternatives}$` : `^(?:${alternatives})$`,
    recursive: false,
  };
}

function uniqueCaseInsensitive(items: string[]): string[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const key = item.toLowerCase();
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export interface DropSummary {
  dirs: PathInfo[];
  files: PathInfo[];
  /** Unique lowercase extensions across the dropped files, in drop order. */
  extensions: string[];
}

export function summarizeDrop(infos: PathInfo[]): DropSummary {
  const dirs = infos.filter((i) => i.kind === 'dir');
  const files = infos.filter((i) => i.kind === 'file');
  const extensions = uniqueCaseInsensitive(files.map((f) => f.extension).filter(Boolean));
  return { dirs, files, extensions };
}

/** Glob preview for the "by type" option, e.g. `*.png;*.jpg`. */
export function typePatternPreview(extensions: string[]): string {
  return extensions.map((e) => `*.${e}`).join(';');
}

/**
 * Turns dropped paths into a category draft:
 * - each directory becomes a `*` glob rule;
 * - files are grouped by parent directory, one rule per directory, matching
 *   either the exact names (regex) or their extensions (glob);
 * - in type mode, extensionless files get an extra exact-name rule;
 * - files whose parent directory was itself dropped are already covered by
 *   that directory's `*` rule and are skipped.
 *
 * Returns null when nothing usable was dropped.
 */
export function buildDraft(infos: PathInfo[], mode: DropMode): CategoryDraft | null {
  const { dirs, files } = summarizeDrop(infos);
  if (dirs.length === 0 && files.length === 0) return null;

  const targets: TargetRule[] = [];
  const dirKeys = new Set<string>();
  for (const d of dirs) {
    const key = dirKey(d.path);
    if (dirKeys.has(key)) continue;
    dirKeys.add(key);
    targets.push({ id: '', dir: d.path, matchType: 'glob', pattern: '*', recursive: false });
  }

  // Group remaining files by parent directory, preserving drop order.
  const groups = new Map<string, { dir: string; files: PathInfo[] }>();
  for (const f of files) {
    const key = dirKey(f.parentDir);
    if (dirKeys.has(key)) continue;
    const group = groups.get(key);
    if (group) group.files.push(f);
    else groups.set(key, { dir: f.parentDir, files: [f] });
  }

  for (const { dir, files: groupFiles } of groups.values()) {
    if (mode === 'file') {
      targets.push(exactNamesRule(dir, uniqueCaseInsensitive(groupFiles.map((f) => f.name))));
      continue;
    }
    const exts = uniqueCaseInsensitive(groupFiles.map((f) => f.extension).filter(Boolean));
    if (exts.length > 0) {
      targets.push({
        id: '',
        dir,
        matchType: 'glob',
        pattern: typePatternPreview(exts),
        recursive: false,
      });
    }
    const bare = uniqueCaseInsensitive(groupFiles.filter((f) => !f.extension).map((f) => f.name));
    if (bare.length > 0) targets.push(exactNamesRule(dir, bare));
  }

  return { name: draftName(dirs, files, mode), targets };
}

function draftName(dirs: PathInfo[], files: PathInfo[], mode: DropMode): string {
  if (dirs.length > 0) return dirs[0].name;
  if (mode === 'type') {
    const exts = uniqueCaseInsensitive(files.map((f) => f.extension).filter(Boolean));
    if (exts.length > 0) {
      const shown = exts.slice(0, MAX_NAME_EXTENSIONS).map((e) => e.toUpperCase()).join('/');
      return exts.length > MAX_NAME_EXTENSIONS ? `${shown}…` : shown;
    }
  }
  return files[0].name;
}
