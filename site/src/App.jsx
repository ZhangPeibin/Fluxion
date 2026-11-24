import { useCallback, useEffect, useState } from "react";

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
  zh: "Eollo — 给 Agent 的 Web3 能力层",
  en: "Eollo — A Web3 Layer for agents",
};

export default function App({ lang }) {
  const t = copy[lang];
  const [domainFilter, setDomainFilter] = useState("all");

  useEffect(() => {
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
    document.title = TITLES[lang];
  }, [lang]);

  const showDomain = useCallback((domain) => {
    setDomainFilter(domain);
    requestAnimationFrame(() => {
      const section = document.getElementById("capabilities");
      if (!section) return;
      const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      section.scrollIntoView({ behavior: reduce ? "auto" : "smooth", block: "start" });
    });
  }, []);

  return (
    <>
      <CursorGlow />
      <a className="skip-link" href="#main">
        {lang === "zh" ? "跳到主要内容" : "Skip to content"}
      </a>
      <Nav t={t} lang={lang} />
      <main id="main">
        <Hero t={t} data={catalog} onSelectDomain={showDomain} />
        <Problems t={t} />
        <Flow t={t} />
        <Capabilities t={t} data={catalog} filter={domainFilter} onFilter={setDomainFilter} />
        <Install t={t} />
        <Skills t={t} />
        <Safety t={t} />
      </main>
      <Footer t={t} lang={lang} />
    </>
  );
}
