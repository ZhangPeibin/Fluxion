import { useEffect, useRef } from "react";

import Reveal from "./Reveal.jsx";
import RouteField from "./RouteField.jsx";
import SectionHeading from "./SectionHeading.jsx";
import { GITHUB_URL } from "../i18n.js";

export default function Flow({ t }) {
  const listRef = useRef(null);

  useEffect(() => {
    const node = listRef.current;
    if (!node) return undefined;
    let frame = 0;

    function update() {
      frame = 0;
      const rect = node.getBoundingClientRect();
      const viewport = window.innerHeight || 1;
      const travelled = viewport * 0.66 - rect.top;
      const progress = Math.min(Math.max(travelled / Math.max(rect.height, 1), 0), 1);
      node.style.setProperty("--flow-progress", progress.toFixed(3));
    }

    function schedule() {
      if (!frame) frame = requestAnimationFrame(update);
    }

    update();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
    };
  }, []);

  return (
    <section className="section flow" id="flow">
      <div className="container flow-layout">
        <div className="flow-intro">
          <SectionHeading eyebrow={t.flow.eyebrow} title={t.flow.title} lead={t.flow.lead} />
          <Reveal as="p" className="callout" delay={160}>
            {t.flow.note}
          </Reveal>
          <Reveal delay={220}>
            <a
              className="text-link"
              href={`${GITHUB_URL}/blob/master/docs/capabilities/swap/README.md`}
              target="_blank"
              rel="noreferrer"
            >
              {t.flow.cta}
              <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                <path d="M4 12 12 4M6 4h6v6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </a>
          </Reveal>
        </div>

        <ol className="flow-list" ref={listRef}>
          <span className="flow-rail" aria-hidden="true">
            <span className="flow-rail-fill" />
          </span>
          {t.steps.map((step, index) => (
            <Reveal as="li" className="flow-step" key={step.id} delay={index * 40}>
              <span className="flow-dot" aria-hidden="true" />
              <div className="flow-step-body">
                <div className="flow-step-head">
                  <span className="flow-step-index">{String(index + 1).padStart(2, "0")}</span>
                  <code className="capability-chip">{step.id}</code>
                </div>
                <h3>{step.title}</h3>
                <p>{step.body}</p>
              </div>
            </Reveal>
          ))}
        </ol>
      </div>

      <div className="container">
        <Reveal className="route-preview" delay={80}>
          <div className="route-preview-head">
            <h3>{t.flow.routeTitle}</h3>
            <p>{t.flow.routeNote}</p>
          </div>
          <RouteField hint={t.hero.canvasHint} />
        </Reveal>
      </div>
    </section>
  );
}
