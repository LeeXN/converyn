/** A path is kept as segments so keys containing dots never collide with nested keys. */
export type ValueRow = { path: string[]; value: unknown };
export function rows(value: unknown, path: string[] = []): ValueRow[] {
  if (value !== null && typeof value === 'object') {
    const entries = Object.entries(value);
    if (entries.length) return entries.flatMap(([key, value]) => rows(value, [...path, key]));
  }
  return path.length ? [{ path, value }] : [];
}
export function replace(value: unknown, path: string[], next: unknown): Record<string, unknown> {
  const result = structuredClone(value) as Record<string, unknown>;
  let target: Record<string, unknown> = result;
  for (const key of path.slice(0, -1)) {
    if (!Object.hasOwn(target, key) || target[key] == null) Object.defineProperty(target, key, { value: {}, enumerable: true, writable: true, configurable: true });
    if (typeof target[key] !== 'object') throw Error('Path crosses a scalar');
    target = target[key] as Record<string, unknown>;
  }
  Object.defineProperty(target, path.at(-1)!, { value: next, enumerable: true, writable: true, configurable: true });
  return result;
}
export function locked(path: string[], immutable: string[]) {
  return immutable.some(key => {
    const parts = key.split('.');
    return parts.every((part, i) => path[i] === part) || path.every((part, i) => parts[i] === part);
  });
}
export function parseValue(text: string, kind: string): unknown {
  if (kind === 'string') return text;
  if (kind === 'boolean') return text === 'true';
  if (kind === 'null') return null;
  if (kind === 'number') {
    if (text.trim() === '' || !Number.isFinite(Number(text))) throw Error('Invalid number');
    return Number(text);
  }
  return JSON.parse(text);
}
/** Keep explicit edits even when equal to workflow defaults: a node may differ.
 * Arrays replace as a unit, matching the CLI/file merge behavior. */
export function override(overrides: Record<string, unknown>, values: Record<string, unknown>, path: string[]): Record<string, unknown> {
  let value: unknown = values;
  let length = 0;
  for (const key of path) {
    if (Array.isArray(value)) break;
    value = (value as Record<string, unknown>)[key];
    length++;
  }
  return replace(overrides, path.slice(0, length), value);
}
