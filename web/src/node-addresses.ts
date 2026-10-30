export type Address = { address: string; interface: string };
/** Presentation preference only: discovery does not establish a management address. */
export function nodeAddresses(value: unknown): Address[] {
  if (!Array.isArray(value)) return [];
  const rows: Address[] = value.filter(v => v && typeof v.address === 'string' && v.address.trim()).map(v => ({ address: v.address, interface: typeof v.interface === 'string' ? v.interface : '' }));
  const rank = (v: Address) => /^(127\.|169\.254\.|::1$|fe80:)/i.test(v.address) ? 10 :
    (/^(lo|veth|docker|cni|flannel|cali|tunl|kube)/.test(v.interface) ? 4 : 0) + (v.address.includes(':') ? 1 : 0);
  rows.sort((a, b) => rank(a) - rank(b) || a.interface.localeCompare(b.interface) || a.address.localeCompare(b.address, undefined, { numeric: true }));
  return rows.filter((row, index) => rows.findIndex(v => v.address === row.address) === index);
}
