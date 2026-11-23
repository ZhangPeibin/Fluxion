import { useMemo, useState } from "react";

import Reveal from "./Reveal.jsx";
import SectionHeading from "./SectionHeading.jsx";
import { GITHUB_URL } from "../i18n.js";

const DOMAIN_ORDER = ["swap", "wallet", "transaction", "signature", "fourmeme", "yield", "position"];

function spotlight(event) {
  const card = event.currentTarget;
  const rect = card.getBoundingClientRect();
  card.style.setProperty("--mx", `${event.clientX - rect.left}px`);
  card.style.setProperty("--my", `${event.clientY - rect.top}px`);
}

export default function Capabilities({ t, data }) {
  const [filter, setFilter] = useState("all");
  const capabilities = data.capabilities;

  const counts = useMemo(() => {
    const map = new Map();
    for (const capability of capabilities) {
      map.set(capability.domain, (map.get(capability.domain) ?? 0) + 1);
    }
    return map;
  }, [capabilities]);

  const visible = filter === "all"
    ? capabilities
    : capabilities.filter((capability) => capability.domain === filter);

  return (
    <section className="section capabilities" id="capabilities">
      <div className="container">
        <SectionHeading
          eyebrow={t.capabilities.eyebrow}
          title={t.capabilities.title}
          lead={t.capabilities.lead}
        />

        <Reveal className="filter-bar" aria-label={t.capabilities.filterLabel}>
          <button
            type="button"
            className={`filter-chip${filter === "all" ? " is-active" : ""}`}
            onClick={() => setFilter("all")}
            aria-pressed={filter === "all"}
          >
            {t.captions.all}
            <span className="filter-count">{capabilities.length}</span>
          </button>
          {DOMAIN_ORDER.filter((domain) => counts.has(domain)).map((domain) => (
            <button
              key={domain}
              type="button"
              className={`filter-chip${filter === domain ? " is-active" : ""}`}
              onClick={() => setFilter(domain)}
              aria-pressed={filter === domain}
            >
              {t.domains[domain].label}
              <span className="filter-count">{counts.get(domain)}</span>
            </button>
          ))}
        </Reveal>

        <div className="cap-grid">
          {visible.map((capability, index) => {
            const domain = t.domains[capability.domain];
            const summary =
              (t.capabilitySummaries && t.capabilitySummaries[capability.id]) ?? capability.summary;
            const docsHref = `${GITHUB_URL}/blob/master/${capability.docs}`;
            return (
              <article
                className="cap-card"
                key={capability.id}
                onPointerMove={spotlight}
                style={{ "--card-index": index }}
              >
                <div className="cap-glow" aria-hidden="true" />
                <header className="cap-head">
                  <span className={`status status-${capability.status}`}>
                    <span className="status-dot" aria-hidden="true" />
                    {t.capabilities.status[capability.status] ?? capability.status}
                  </span>
                  <span className={`exec exec-${capability.execution}`}>
                    {t.capabilities.execution[capability.execution] ?? capability.execution}
                  </span>
                </header>

                <h3 className="cap-id">{capability.id}</h3>
                <p className="cap-domain">{domain?.label ?? capability.domain}</p>
                <p className="cap-summary">{summary}</p>

                <footer className="cap-foot">
                  <span className="chain-list" aria-label={t.capabilities.chainsLabel}>
                    {capability.chains.map((chain) => (
                      <span className={`chain chain-${chain}`} key={chain}>
                        {chain === "ethereum" ? "ETH" : chain === "bsc" ? "BSC" : chain}
                      </span>
                    ))}
                  </span>
                  <a className="cap-link" href={docsHref} target="_blank" rel="noreferrer">
                    {t.captions.docsLink}
                    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                      <path d="M4 12 12 4M6 4h6v6" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </a>
                </footer>
              </article>
            );
          })}
        </div>
      </div>
    </section>
  );
}
