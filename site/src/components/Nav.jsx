import { useEffect, useState } from "react";

import { GITHUB_URL } from "../i18n.js";

function Mark() {
  return (
    <svg className="brand-mark" viewBox="0 0 32 32" width="30" height="30" aria-hidden="true">
      <path
        d="M7 23 16 8l9 15"
        fill="none"
        stroke="url(#brandGradient)"
        strokeWidth="2.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="16" cy="8" r="3" fill="#22d3ee" />
      <circle cx="7" cy="23" r="2.2" fill="#4d9dff" />
      <circle cx="25" cy="23" r="2.2" fill="#3fb950" />
      <defs>
        <linearGradient id="brandGradient" x1="7" y1="23" x2="25" y2="8" gradientUnits="userSpaceOnUse">
          <stop stopColor="#4d9dff" />
          <stop offset="1" stopColor="#22d3ee" />
        </linearGradient>
      </defs>
    </svg>
  );
}

export default function Nav({ t, lang }) {
  const [scrolled, setScrolled] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    function onScroll() {
      setScrolled(window.scrollY > 12);
    }
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    document.body.classList.toggle("menu-open", open);
    return () => document.body.classList.remove("menu-open");
  }, [open]);

  return (
    <header className={`nav${scrolled ? " is-scrolled" : ""}${open ? " is-open" : ""}`}>
      <div className="nav-inner">
        <a className="brand" href="#top" aria-label={t.nav.brand} onClick={() => setOpen(false)}>
          <Mark />
          <span className="brand-text">
            <span className="brand-name">{t.nav.brand}</span>
            <span className="brand-tagline">{t.nav.tagline}</span>
          </span>
        </a>

        <nav className="nav-links" aria-label={lang === "en" ? "Primary" : "主导航"}>
          {t.nav.links.map((link) => (
            <a key={link.href} href={link.href} onClick={() => setOpen(false)}>
              {link.label}
            </a>
          ))}
        </nav>

        <div className="nav-actions">
          <a className="icon-link" href={GITHUB_URL} target="_blank" rel="noreferrer" aria-label="GitHub">
            <svg viewBox="0 0 16 16" width="17" height="17" aria-hidden="true">
              <path
                fill="currentColor"
                d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.4 7.4 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
              />
            </svg>
          </a>
          <a className="lang-link" href={t.captions.langHref} hrefLang={lang === "zh" ? "en" : "zh-CN"}>
            {t.captions.langSwitch}
          </a>
          <a className="button button-small" href="#install" onClick={() => setOpen(false)}>
            {t.nav.cta}
          </a>
          <button
            type="button"
            className="menu-toggle"
            aria-expanded={open}
            aria-label={t.captions.openMenu}
            onClick={() => setOpen((value) => !value)}
          >
            <span />
            <span />
          </button>
        </div>
      </div>

      <nav className="nav-drawer" aria-label={lang === "en" ? "Mobile" : "移动导航"}>
        {t.nav.links.map((link) => (
          <a key={link.href} href={link.href} onClick={() => setOpen(false)}>
            {link.label}
          </a>
        ))}
        <a href={t.captions.langHref} hrefLang={lang === "zh" ? "en" : "zh-CN"}>
          {t.captions.langSwitch}
        </a>
      </nav>
    </header>
  );
}
