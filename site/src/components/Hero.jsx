import CopyButton from "./CopyButton.jsx";
import RouteField from "./RouteField.jsx";
import { GITHUB_URL, INSTALL_COMMAND } from "../i18n.js";

export default function Hero({ t, capabilityCount, domainCount }) {
  const stats = t.hero.stats.map((stat) => {
    if (stat.key === "capabilities") return { ...stat, value: String(capabilityCount) };
    if (stat.key === "domains") return { ...stat, value: String(domainCount) };
    return stat;
  });

  return (
    <section className="hero" id="top">
      <div className="hero-aurora" aria-hidden="true" />
      <div className="hero-grid" aria-hidden="true" />

      <div className="hero-inner">
        <div className="hero-copy">
          <p className="eyebrow intro" style={{ "--intro-delay": "40ms" }}>
            <span className="pulse-dot" aria-hidden="true" />
            {t.hero.eyebrow}
          </p>

          <h1 className="hero-title intro" style={{ "--intro-delay": "110ms" }}>
            <span>{t.hero.title[0]}</span>{" "}
            <span className="gradient-text">{t.hero.title[1]}</span>
          </h1>

          <p className="hero-lead intro" style={{ "--intro-delay": "190ms" }}>
            {t.hero.lead}
          </p>

          <div className="hero-command intro" style={{ "--intro-delay": "260ms" }}>
            <span className="hero-command-label">{t.hero.installLabel}</span>
            <div className="command-pill">
              <code>
                <span className="prompt">$</span> {INSTALL_COMMAND}
              </code>
              <CopyButton
                text={INSTALL_COMMAND}
                label={t.captions.copy}
                copiedLabel={t.captions.copied}
                className="copy-button-inline"
              />
            </div>
          </div>

          <div className="hero-ctas intro" style={{ "--intro-delay": "330ms" }}>
            <a className="button" href="#install">
              {t.hero.primary}
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
                <path d="M3 8h9M8.5 4.5 12 8l-3.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </a>
            <a className="button button-ghost" href={GITHUB_URL} target="_blank" rel="noreferrer">
              {t.hero.secondary}
            </a>
          </div>

          <dl className="hero-stats intro" style={{ "--intro-delay": "400ms" }}>
            {stats.map((stat) => (
              <div key={stat.label}>
                <dt>{stat.value}</dt>
                <dd>{stat.label}</dd>
              </div>
            ))}
          </dl>
        </div>

        <div className="hero-visual intro" style={{ "--intro-delay": "240ms" }}>
          <RouteField hint={t.hero.canvasHint} />
        </div>
      </div>
    </section>
  );
}
