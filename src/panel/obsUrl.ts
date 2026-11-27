import type { Mode } from "../lib/types";

/** Public name that always resolves to 127.0.0.1 (TikTok LIVE Studio rejects localhost and raw IPs). */
export const LOOPBACK_DOMAIN = "localtest.me";

function overlayUrl(host: string, port: number, mode?: Mode): string {
  const authority = port === 80 ? host : `${host}:${port}`;
  const base = `http://${authority}/overlay`;
  return mode ? `${base}?mode=${mode}` : base;
}

export function obsUrl(port: number, mode?: Mode): string {
  return mode ? `http://127.0.0.1:${port}/overlay?mode=${mode}` : `http://127.0.0.1:${port}/overlay`;
}

/** URL for TikTok LIVE Studio's Link source: a dotted domain, and no port when the app runs on 80. */
export function tiktokUrl(port: number, mode?: Mode): string {
  return overlayUrl(LOOPBACK_DOMAIN, port, mode);
}
