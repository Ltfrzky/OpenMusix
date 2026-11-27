import "@fontsource-variable/inter";
import "@fontsource-variable/outfit";
import "@fontsource-variable/lora";
import "material-symbols/rounded.css";
import "./theme.css";
import { mount } from "svelte";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });
