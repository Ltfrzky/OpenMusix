import type { Snapshot } from "./types";

/** Connect to the overlay socket and keep reconnecting (500 ms doubling to 5 s). Returns dispose. */
export function connectSnapshots(url: string, onSnapshot: (s: Snapshot) => void, onStatus?: (connected: boolean) => void): () => void {
  let ws: WebSocket | null = null;
  let delay = 500;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;

  const open = () => {
    if (disposed) return;
    ws = new WebSocket(url);
    ws.onopen = () => {
      delay = 500;
      onStatus?.(true);
    };
    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(String(ev.data));
        if (msg?.type === "snapshot") onSnapshot(msg.data as Snapshot);
      } catch {
        /* ignore malformed frames */
      }
    };
    ws.onclose = () => {
      onStatus?.(false);
      if (disposed) return;
      timer = setTimeout(open, delay);
      delay = Math.min(delay * 2, 5000);
    };
    ws.onerror = () => ws?.close();
  };

  open();
  return () => {
    disposed = true;
    if (timer) clearTimeout(timer);
    ws?.close();
  };
}
