import { useRef, useState } from "react";

import CodeBlock from "./CodeBlock.jsx";
import { GITHUB_URL } from "../i18n.js";
import { MANAGED_REMOTE_URL } from "../config.js";

const HAS_MANAGED = Boolean(MANAGED_REMOTE_URL);

export default function RemoteCard({ t }) {
  const remote = t.install.remote;
  const [active, setActive] = useState(HAS_MANAGED ? "managed" : "selfHost");
  const tabRefs = useRef([]);

  const tabs = [
    { id: "managed", label: remote.tabs.managed },
    { id: "selfHost", label: remote.tabs.selfHost },
  ];

  function onKeyDown(event) {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const index = tabs.findIndex((tab) => tab.id === active);
    const next =
      event.key === "ArrowRight"
        ? (index + 1) % tabs.length
        : (index - 1 + tabs.length) % tabs.length;
    setActive(tabs[next].id);
    tabRefs.current[next]?.focus();
  }

  return (
    <div className="remote-block">
      <div className="remote-intro">
        <h3>{remote.title}</h3>
        <p>{remote.lead}</p>
      </div>

      <div className="terminal-card">
        <div className="tabs" role="tablist" aria-label={remote.title} onKeyDown={onKeyDown}>
          {tabs.map((tab, index) => (
            <button
              key={tab.id}
              ref={(node) => {
                tabRefs.current[index] = node;
              }}
              type="button"
              role="tab"
              id={`remote-tab-${tab.id}`}
              aria-selected={active === tab.id}
              aria-controls={`remote-panel-${tab.id}`}
              tabIndex={active === tab.id ? 0 : -1}
              className={`tab${active === tab.id ? " is-active" : ""}`}
              onClick={() => setActive(tab.id)}
            >
              {tab.label}
              {tab.id === "managed" && !HAS_MANAGED && (
                <span className="tab-badge">{remote.badge}</span>
              )}
            </button>
          ))}
        </div>

        <div
          className="tab-panel"
          role="tabpanel"
          id={`remote-panel-${active}`}
          aria-labelledby={`remote-tab-${active}`}
        >
          {active === "managed" && HAS_MANAGED && (
            <>
              <CodeBlock
                code={`EOLLO_REMOTE_URL=${MANAGED_REMOTE_URL} eollo mcp`}
                path="shell"
                copyLabel={t.captions.copy}
                copiedLabel={t.captions.copied}
              />
              <ul className="remote-notes">
                <li>{remote.managed.scope}</li>
                <li>{remote.managed.authNote}</li>
              </ul>
              <div className="remote-links">
                <a
                  className="text-link"
                  href={`${GITHUB_URL}/blob/master/docs/MCP.md#remote-service`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {remote.managed.docs}
                </a>
              </div>
            </>
          )}

          {active === "managed" && !HAS_MANAGED && (
            <div className="remote-pending">
              <p>{remote.managed.pending}</p>
              <button type="button" className="button button-small" onClick={() => setActive("selfHost")}>
                {remote.managed.pendingCta}
              </button>
            </div>
          )}

          {active === "selfHost" && (
            <>
              <CodeBlock
                code={`${remote.selfHost.installCommand}\n${remote.selfHost.serveCommand}`}
                path="shell"
                copyLabel={t.captions.copy}
                copiedLabel={t.captions.copied}
              />
              <ul className="remote-notes">
                <li>{remote.selfHost.secretNote}</li>
                <li>{remote.selfHost.warmupNote}</li>
              </ul>
              <div className="remote-links">
                <a
                  className="text-link"
                  href={`${GITHUB_URL}/blob/master/docs/MCP.md#remote-service`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {remote.managed.docs}
                </a>
                <a
                  className="text-link"
                  href={`${GITHUB_URL}/blob/master/docs/CONFIGURATION.md`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {remote.selfHost.docs}
                </a>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
