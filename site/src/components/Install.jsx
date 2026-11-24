import { useRef, useState } from "react";

import CodeBlock from "./CodeBlock.jsx";
import CopyButton from "./CopyButton.jsx";
import RemoteCard from "./RemoteCard.jsx";
import Reveal from "./Reveal.jsx";
import SectionHeading from "./SectionHeading.jsx";
import { INSTALL_COMMAND, MCP_COMMAND, MCP_CONFIGS } from "../i18n.js";

const TABS = [
  { id: "claude", label: "Claude Code" },
  { id: "codex", label: "Codex" },
  { id: "opencode", label: "OpenCode" },
];

export default function Install({ t }) {
  const [active, setActive] = useState("claude");
  const tabRefs = useRef([]);
  const config = MCP_CONFIGS[active];

  function onKeyDown(event) {
    const index = TABS.findIndex((tab) => tab.id === active);
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const next = event.key === "ArrowRight"
      ? (index + 1) % TABS.length
      : (index - 1 + TABS.length) % TABS.length;
    setActive(TABS[next].id);
    tabRefs.current[next]?.focus();
  }

  const commands = [INSTALL_COMMAND, MCP_COMMAND, null];

  return (
    <section className="section install" id="install">
      <div className="container">
        <SectionHeading eyebrow={t.install.eyebrow} title={t.install.title} lead={t.install.lead} />

        <div className="install-steps">
          {t.install.steps.map((step, index) => (
            <Reveal className="install-step" key={step.title} delay={index * 80}>
              <span className="install-number">{String(index + 1).padStart(2, "0")}</span>
              <h3>{step.title}</h3>
              <p>{step.body}</p>
              {commands[index] ? (
                <div className="mini-command">
                  <code>{commands[index]}</code>
                  <CopyButton
                    text={commands[index]}
                    label={t.captions.copy}
                    copiedLabel={t.captions.copied}
                    className="copy-button-inline"
                  />
                </div>
              ) : (
                <a className="text-link install-jump" href="#mcp">
                  {t.install.mcpTitle}
                  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                    <path d="M8 3v10M3.5 8.5 8 13l4.5-4.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                </a>
              )}
            </Reveal>
          ))}
        </div>

        <div className="mcp-block" id="mcp">
          <Reveal className="mcp-intro">
            <h3>{t.install.mcpTitle}</h3>
            <p>{t.install.mcpLead}</p>
          </Reveal>

          <Reveal className="terminal-card" delay={80}>
            <div className="tabs" role="tablist" aria-label={t.install.mcpTitle} onKeyDown={onKeyDown}>
              {TABS.map((tab, index) => (
                <button
                  key={tab.id}
                  ref={(node) => {
                    tabRefs.current[index] = node;
                  }}
                  type="button"
                  role="tab"
                  id={`tab-${tab.id}`}
                  aria-selected={active === tab.id}
                  aria-controls={`panel-${tab.id}`}
                  tabIndex={active === tab.id ? 0 : -1}
                  className={`tab${active === tab.id ? " is-active" : ""}`}
                  onClick={() => setActive(tab.id)}
                >
                  {tab.label}
                </button>
              ))}
            </div>
            <div
              className="tab-panel"
              role="tabpanel"
              id={`panel-${active}`}
              aria-labelledby={`tab-${active}`}
            >
              <CodeBlock
                code={config.code}
                path={config.path}
                copyLabel={t.captions.copy}
                copiedLabel={t.captions.copied}
              />
            </div>
          </Reveal>
        </div>

        <Reveal className="remote-wrap" delay={120}>
          <RemoteCard t={t} />
        </Reveal>
      </div>
    </section>
  );
}
