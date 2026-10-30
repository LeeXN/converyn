import { useEffect, useState } from 'react';
// The loader identity is the resource boundary. A manual refresh keeps only that
// resource's last successful snapshot; a different loader never exposes it.
export function usePoll<T>(
  load: (signal: AbortSignal) => Promise<T>,
  revision: number,
  enabled = true,
) {
  const [state, setState] = useState<{
    load: typeof load;
    data?: T;
    error: string;
    updated: number;
    next: number;
    loading: boolean;
  }>({ load, error: '', updated: 0, next: 0, loading: true });
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (!enabled) {
      setState({ load, error: '', updated: 0, next: 0, loading: false });
      return;
    }
    let active = true,
      timer: ReturnType<typeof setTimeout> | undefined;
    let delay = 5000;
    const controller = new AbortController();
    setState((s) =>
      s.load === load
        ? { ...s, loading: true, next: 0 }
        : { load, error: '', updated: 0, next: 0, loading: true },
    );
    const refresh = async () => {
      try {
        const data = await load(controller.signal);
        if (!active) return;
        delay = 5000;
        setState({
          load,
          data,
          error: '',
          updated: Date.now(),
          next: Date.now() + delay,
          loading: false,
        });
      } catch (e) {
        if (!active) return;
        delay = Math.min(delay * 2, 30000);
        setState((s) => ({
          ...s,
          load,
          error: String(e),
          next: Date.now() + delay,
          loading: false,
        }));
      }
      if (active) timer = setTimeout(refresh, delay);
    };
    void refresh();
    return () => {
      active = false;
      controller.abort();
      clearTimeout(timer);
    };
  }, [load, revision, enabled]);
  const value =
    enabled && state.load === load
      ? state
      : { data: undefined, error: '', updated: 0, next: 0, loading: enabled };
  return { ...value, fresh: value.updated > 0 && !value.error && now - value.updated < 15000 };
}
