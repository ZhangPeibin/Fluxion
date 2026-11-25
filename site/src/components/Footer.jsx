export default function Footer({ t, lang }) {
  return (
    <footer className="footer">
      <div className="container footer-inner">
        <div className="footer-brand">
          <a className="brand" href="#top">
            <span className="brand-name">{t.nav.brand}</span>
          </a>
          <p>{t.footer.blurb}</p>
          <div className="footer-meta">
            <a href={t.captions.langHref} hrefLang={lang === "zh" ? "en" : "zh-CN"}>
              {t.captions.langSwitch}
            </a>
          </div>
        </div>

        <div className="footer-columns">
          {t.footer.columns.map((column) => (
            <div key={column.title}>
              <h3>{column.title}</h3>
              <ul>
                {column.links.map((link) => (
                  <li key={link.label}>
                    <a href={link.href} target="_blank" rel="noreferrer">
                      {link.label}
                    </a>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>
      <div className="container footer-bottom">
        <span>{t.footer.copyright}</span>
        <a className="back-top" href="#top">
          <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
            <path d="M8 13V3M3.5 7.5 8 3l4.5 4.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </a>
      </div>
    </footer>
  );
}
