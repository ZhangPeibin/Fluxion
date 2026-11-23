import Reveal from "./Reveal.jsx";
import SectionHeading from "./SectionHeading.jsx";
import { GITHUB_URL } from "../i18n.js";

export default function Safety({ t }) {
  return (
    <section className="section safety" id="safety">
      <div className="container safety-layout">
        <div className="safety-intro">
          <SectionHeading eyebrow={t.safety.eyebrow} title={t.safety.title} lead={t.safety.lead} />
          <Reveal as="p" className="safety-disclaimer" delay={160}>
            {t.safety.disclaimer}
          </Reveal>
          <Reveal delay={220}>
            <a
              className="button button-ghost"
              href={`${GITHUB_URL}/blob/master/docs/capabilities/transaction/ASSURANCE.md`}
              target="_blank"
              rel="noreferrer"
            >
              {t.safety.cta}
            </a>
          </Reveal>
        </div>

        <div className="safety-grid">
          {t.safety.items.map((item, index) => (
            <Reveal className="safety-item" key={item.title} delay={index * 70}>
              <span className="safety-check" aria-hidden="true">
                <svg viewBox="0 0 16 16" width="12" height="12">
                  <path d="M3.5 8.5l3 3 6-6.5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </span>
              <h3>{item.title}</h3>
              <p>{item.body}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
