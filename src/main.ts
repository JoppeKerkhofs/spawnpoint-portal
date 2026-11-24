import { createApp } from "vue";
import App from "./App.vue";
import { initLook } from "./logic/look";
import { os } from "./logic/os";
import "./style.css";

initLook();
document.documentElement.dataset.os = os;
createApp(App).mount("#app");
