import "@fontsource-variable/inter";
import "@fontsource-variable/outfit";
import "@fontsource-variable/lora";
import { mount } from "svelte";
import OverlayApp from "./OverlayApp.svelte";
import type { Mode } from "../lib/types";

const params = new URLSearchParams(location.search);
const m = params.get("mode");
const pinnedMode: Mode | undefined = m === "focus" || m === "widget" ? m : undefined;
const float = params.get("float") === "1";
const served = location.protocol === "http:" && location.hostname === "127.0.0.1";
const port = params.get("port") ?? "7878";
const origin = served ? location.host : `127.0.0.1:${port}`;

mount(OverlayApp, {
  target: document.getElementById("app")!,
  props: { wsUrl: `ws://${origin}/ws`, artBase: served ? "" : `http://${origin}`, pinnedMode, float },
});
