import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App.jsx";
import "./styles.css";

const root = document.getElementById("root");
const lang = root?.dataset.lang === "en" ? "en" : "zh";

createRoot(root).render(
  <StrictMode>
    <App lang={lang} />
  </StrictMode>,
);
