import { useEffect, useMemo } from "react";

import Capabilities from "./components/Capabilities.jsx";
import CursorGlow from "./components/CursorGlow.jsx";
import Flow from "./components/Flow.jsx";
import Footer from "./components/Footer.jsx";
import Hero from "./components/Hero.jsx";
import Install from "./components/Install.jsx";
import Nav from "./components/Nav.jsx";
import Problems from "./components/Problems.jsx";
import Safety from "./components/Safety.jsx";
import Skills from "./components/Skills.jsx";
import catalog from "./data/capabilities.json";
import { copy } from "./i18n.js";

const TITLES = {
  zh: "Fluxion — 给 Agent 的 Web3 能力层",
  en: "Fluxion — a Web3 capability layer for agents",
};

export default function App({ lang }) {
  const t = copy[lang];

  useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
    document.title = TITLES[lang];
  }, [lang]);

  const domainCount = useMemo(
    () => new Set(catalog.capabilities.map((capability) => capability.domain)).size,
    [],
  );

  return (
    <>
      <CursorGlow />
      <a className="skip-link" href="#main">
        {lang === "zh" ? "跳到主要内容" : "Skip to content"}
      </a>
      <Nav t={t} lang={lang} />
      <main id="main">
        <Hero t={t} capabilityCount={catalog.capabilities.length} domainCount={domainCount} />
        <Problems t={t} />
        <Flow t={t} />
        <Capabilities t={t} data={catalog} />
        <Install t={t} />
        <Skills t={t} />
        <Safety t={t} />
      </main>
      <Footer t={t} lang={lang} />
    </>
  );
}
