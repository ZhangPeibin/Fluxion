import CapabilityMap from "./CapabilityMap.jsx";
import CopyButton from "./CopyButton.jsx";
import { INSTALL_COMMAND } from "../i18n.js";

export default function Hero({ t, data, onSelectDomain }) {
  const stats = t.hero.stats.map((stat) => {
    if (stat.key === "capabilities") return { ...stat, value: String(data.capabilities.length) };
    if (stat.key === "domains") {
      return { ...stat, value: String(new Set(data.capabilities.map((capability) => capability.domain)).size) };
    }
    return stat;
  });

  return (
    <section className="hero" id="top">
      <div className="hero-inner">
        <div className="hero-copy">
          <p className="eyebrow intro" style={{ "--intro-delay": "40ms" }}>
            <span className="pulse-dot" aria-hidden="true" />
            {t.hero.eyebrow}
          </p>

          <h1 className="hero-title intro" style={{ "--intro-delay": "110ms" }}>
            <span className="title-line">
              {t.hero.title.before}
              {!t.hero.title.breakAfterHighlight && (
                <br className="title-break" aria-hidden="true" />
              )}
              <span className="gradient-text">{t.hero.title.highlight}</span>
              {t.hero.title.breakAfterHighlight && (
                <br className="title-break" aria-hidden="true" />
              )}
              {t.hero.title.after}
            </span>
          </h1>

          <p className="hero-tagline">
            <span className="tagline-line intro" style={{ "--intro-delay": "190ms" }}>
              {t.hero.tagline.line1}
            </span>
            <span className="tagline-line intro" style={{ "--intro-delay": "420ms" }}>
              {t.hero.tagline.line2Before}
              <span className="gradient-text">{t.hero.tagline.highlight}</span>
              {t.hero.tagline.line2After}
            </span>
          </p>

          <div className="hero-command intro" style={{ "--intro-delay": "560ms" }}>
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

          <a className="hero-install-link intro" style={{ "--intro-delay": "600ms" }} href="#install">
            {t.hero.installCta}
            <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
              <path d="M3 8h9M8.5 4.5 12 8l-3.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </a>

          <dl className="hero-stats intro" style={{ "--intro-delay": "660ms" }}>
            {stats.map((stat) => (
              <div key={stat.label}>
                <dt>{stat.value}</dt>
                <dd>{stat.label}</dd>
              </div>
            ))}
          </dl>
        </div>
      </div>

      <CapabilityMap t={t} data={data} onSelectDomain={onSelectDomain} />
      <div className="hero-scrim" aria-hidden="true" />

      <a className="hero-scroll" href="#problems">
        <span>{t.hero.scrollCue}</span>
        <span className="scroll-line" aria-hidden="true" />
      </a>
    </section>
  );
}
