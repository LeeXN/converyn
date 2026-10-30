export function isTerminal(status: string) {
  return ['succeeded', 'failed', 'canceled'].includes(status);
}
/** Wall time includes queueing and pauses; old completed runs need both timestamps. */
export function elapsedMilliseconds(submitted: number | null | undefined, finished: number | null | undefined, status: string, now: number): number | undefined {
  if (submitted == null || !Number.isFinite(submitted) || submitted < 0) return undefined;
  const end = finished ?? (isTerminal(status) ? undefined : now);
  if (end == null || !Number.isFinite(end) || end < submitted) return undefined;
  return end - submitted;
}
export function durationText(milliseconds?: number): string {
  if (milliseconds === undefined) return '—';
  const seconds = Math.floor(milliseconds / 1000);
  if (seconds < 60) return seconds + ' s';
  if (seconds < 3600) return Math.floor(seconds / 60) + ' m ' + seconds % 60 + ' s';
  return Math.floor(seconds / 3600) + ' h ' + Math.floor(seconds % 3600 / 60) + ' m ' + seconds % 60 + ' s';
}
