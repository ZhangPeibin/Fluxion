import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App.jsx";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./styles.css";
import "./graphite.css";

const root = document.getElementById("root");
const lang = root?.dataset.lang === "en" ? "en" : "zh";

if (new URLSearchParams(window.location.search).has("snap")) {
  document.documentElement.classList.add("snap");
}

createRoot(root).render(
  <StrictMode>
    <App lang={lang} />
  </StrictMode>,
);
