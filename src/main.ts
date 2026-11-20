import { createApp } from "vue";
import App from "./App.vue";
import { initLook } from "./logic/look";
import "./style.css";

initLook();
createApp(App).mount("#app");
