import { getCurrentWindow } from "@tauri-apps/api/window";
import { createApp } from "vue";
import App from "./App.vue";
import { initLook } from "./logic/look";
import { os } from "./logic/os";
import "./style.css";

initLook();
document.documentElement.dataset.os = os;
createApp(App).mount("#app");
// The window starts hidden and shows once the page is drawn in its look, so it never opens blank.
requestAnimationFrame(() => requestAnimationFrame(() => getCurrentWindow().show()));
