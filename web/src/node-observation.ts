import type { crates_core_orchestration_machine_types_NodeObservation as Node } from './api-types';
export type ObservationKind = 'inventory' | 'resources';
const sizeLimit = 'observation size limit exceeded';

/** Completeness and collection age are independent; an omitted list is not a collection failure. */
export function observationStatus(node: Node, kind: ObservationKind, now = Date.now()) {
    const snapshot = node[kind];
    if (!snapshot) return undefined;
    const checked = kind === 'inventory' ? node.inventory_checked_ms ?? snapshot.collected_ms : snapshot.collected_ms;
    const elapsed = Math.max(0, now - checked);
    const unit: 'seconds' | 'minutes' | 'hours' | 'days' = elapsed < 60000 ? 'seconds' : elapsed < 3600000 ? 'minutes' : elapsed < 86400000 ? 'hours' : 'days';
    const divisor = { seconds: 1000, minutes: 60000, hours: 3600000, days: 86400000 }[unit];
    const omitted = Object.entries(snapshot.errors ?? {}).filter(([, error]) => error.startsWith(sizeLimit)).map(([key]) => key);
    const errors = Object.entries(snapshot.errors ?? {}).filter(([, error]) => !error.startsWith(sizeLimit));
    return { checked, stale: elapsed > (kind === 'inventory' ? 600000 : 60000), unit, count: Math.floor(elapsed / divisor), omitted, errors, truncated: !!snapshot.truncated || omitted.length > 0 };
}
