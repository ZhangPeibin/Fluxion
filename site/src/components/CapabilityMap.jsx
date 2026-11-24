import { useEffect, useMemo, useRef, useState } from "react";

import { GITHUB_URL } from "../i18n.js";

const DOMAIN_ORDER = ["swap", "wallet", "transaction", "signature", "fourmeme", "yield", "position"];

const DOMAIN_COLOR = {
  swap: "#4d9dff",
  wallet: "#22d3ee",
  transaction: "#34d399",
  signature: "#f5c451",
  fourmeme: "#f472b6",
  yield: "#2dd4bf",
  position: "#fb923c",
};

const DESKTOP = { w: 1200, h: 760 };
const COMPACT = { w: 600, h: 900, cx: 300, cy: 470, rx: 188, ry: 276 };

function useMediaQuery(query) {
  const [matches, setMatches] = useState(
    () => typeof window !== "undefined" && window.matchMedia(query).matches,
  );

  useEffect(() => {
    const media = window.matchMedia(query);
    const update = () => setMatches(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, [query]);

  return matches;
}

function hash(value) {
  let result = 0;
  for (let index = 0; index < value.length; index += 1) {
    result = (result * 31 + value.charCodeAt(index)) | 0;
  }
  return result;
}

function curve(a, b, seed, strength = 0.16) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length = Math.hypot(dx, dy) || 1;
  const direction = (seed + hash(`${a.x}:${b.y}`)) % 2 === 0 ? 1 : -1;
  const bend = Math.min(48, length * strength) * direction;
  const cx = (a.x + b.x) / 2 - (dy / length) * bend;
  const cy = (a.y + b.y) / 2 + (dx / length) * bend;
  return `M ${a.x.toFixed(1)} ${a.y.toFixed(1)} Q ${cx.toFixed(1)} ${cy.toFixed(1)} ${b.x.toFixed(1)} ${b.y.toFixed(1)}`;
}

function buildLayout(capabilities, compact) {
  const bounds = compact ? COMPACT : DESKTOP;
  const byDomain = new Map();
  for (const capability of capabilities) {
    if (!byDomain.has(capability.domain)) byDomain.set(capability.domain, []);
    byDomain.get(capability.domain).push(capability);
  }

  const hubs = {};
  const nodes = {};

  DOMAIN_ORDER.forEach((domain, index) => {
    let x;
    let y;
    if (compact) {
      const angle = -Math.PI / 2 + index * ((Math.PI * 2) / DOMAIN_ORDER.length) + 0.08;
      const scale = index % 2 === 0 ? 1 : 0.9;
      x = COMPACT.cx + Math.cos(angle) * COMPACT.rx * scale;
      y = COMPACT.cy + Math.sin(angle) * COMPACT.ry * scale;
    } else {
      const angle = -Math.PI / 2 + index * ((Math.PI * 2) / DOMAIN_ORDER.length);
      x = 600 + Math.cos(angle) * 470;
      y = 300 + Math.sin(angle) * 180;
    }

    const list = byDomain.get(domain) ?? [];
    const ring = compact ? 0 : list.length <= 1 ? 20 : Math.max(36, 16 + list.length * 4.4);
    hubs[domain] = { x, y, count: list.length, ring };

    if (compact) return;

    list.forEach((capability, position) => {
      const nodeAngle =
        -Math.PI / 2 + (position * Math.PI * 2) / Math.max(list.length, 1) + index * 0.55;
      nodes[capability.id] = {
        x: x + Math.cos(nodeAngle) * ring * 1.06,
        y: y + Math.sin(nodeAngle) * ring * 0.84,
      };
    });
  });

  const wallet = hubs.wallet;
  const ghost = wallet ? { x: wallet.x - 74, y: wallet.y + 58 } : null;

  return { bounds, hubs, nodes, ghost };
}

function positionFor(id, nodes, ghost) {
  if (id === "wallet_session") return ghost;
  return nodes[id];
}

function buildWorkflowEdges(workflows, nodes, ghost) {
  const map = new Map();
  for (const workflow of workflows) {
    for (let index = 0; index < workflow.steps.length - 1; index += 1) {
      const from = workflow.steps[index];
      const to = workflow.steps[index + 1];
      const a = positionFor(from, nodes, ghost);
      const b = positionFor(to, nodes, ghost);
      if (!a || !b) continue;
      const key = `${from}->${to}`;
      const edge = map.get(key) ?? { from, to, a, b, workflows: [] };
      if (!edge.workflows.includes(workflow.id)) edge.workflows.push(workflow.id);
      map.set(key, edge);
    }
  }
  return [...map.values()].map((edge, index) => ({
    ...edge,
    key: `${edge.from}->${edge.to}`,
    path: curve(edge.a, edge.b, index),
  }));
}

function buildRequiresEdges(capabilities, nodes, ghost) {
  const edges = [];
  for (const capability of capabilities) {
    for (const required of capability.requires ?? []) {
      const a = positionFor(required, nodes, ghost);
      const b = nodes[capability.id];
      if (!a || !b) continue;
      edges.push({
        key: `${required}->${capability.id}`,
        from: required,
        to: capability.id,
        a,
        b,
        path: curve(a, b, edges.length, 0.1),
      });
    }
  }
  return edges;
}

function domainEdges(workflows, capabilities) {
  const domainOf = new Map(capabilities.map((capability) => [capability.id, capability.domain]));
  const map = new Map();
  for (const workflow of workflows) {
    const domains = workflow.steps.map((step) => domainOf.get(step)).filter(Boolean);
    for (let index = 0; index < domains.length - 1; index += 1) {
      const from = domains[index];
      const to = domains[index + 1];
      if (from === to) continue;
      const key = `${from}->${to}`;
      const edge = map.get(key) ?? { from, to, workflows: [] };
      if (!edge.workflows.includes(workflow.id)) edge.workflows.push(workflow.id);
      map.set(key, edge);
    }
  }
  return [...map.values()];
}

function CapabilityDetail({ t, capability, onClose, onSelectDomain }) {
  const summary =
    (t.capabilitySummaries && t.capabilitySummaries[capability.id]) ?? capability.summary;
  return (
    <div
      className="map-detail"
      role="dialog"
      aria-label={capability.id}
    >
      <div className="map-detail-head">
        <code>{capability.id}</code>
        <button type="button" className="map-detail-close" onClick={onClose} aria-label={t.map.close}>
          <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
            <path d="m4 4 8 8M12 4l-8 8" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </button>
      </div>
      <div className="map-detail-meta">
        <span className={`status status-${capability.status}`}>
          <span className="status-dot" aria-hidden="true" />
          {t.capabilities.status[capability.status] ?? capability.status}
        </span>
        <span className={`exec exec-${capability.execution}`}>
          {t.capabilities.execution[capability.execution] ?? capability.execution}
        </span>
        {capability.chains.map((chain) => (
          <span className={`chain chain-${chain}`} key={chain}>
            {chain === "ethereum" ? "ETH" : chain === "bsc" ? "BSC" : chain}
          </span>
        ))}
      </div>
      <p>{summary}</p>
      <div className="map-detail-foot">
        <button
          type="button"
          className="button button-small"
          onClick={() => onSelectDomain(capability.domain)}
        >
          {t.map.viewInCatalog}
        </button>
        <a
          className="text-link"
          href={`${GITHUB_URL}/blob/master/${capability.docs}`}
          target="_blank"
          rel="noreferrer"
        >
          {t.captions.docsLink}
        </a>
      </div>
    </div>
  );
}

export default function CapabilityMap({ t, data, onSelectDomain }) {
  const compact = useMediaQuery("(max-width: 700px)");
  const reducedMotion = useMediaQuery("(prefers-reduced-motion: reduce)");
  const layerRef = useRef(null);

  const { capabilities, workflows } = data;
  const layout = useMemo(() => buildLayout(capabilities, compact), [capabilities, compact]);
  const workflowEdges = useMemo(
    () => (compact ? [] : buildWorkflowEdges(workflows, layout.nodes, layout.ghost)),
    [compact, workflows, layout],
  );
  const requiresEdges = useMemo(
    () => (compact ? [] : buildRequiresEdges(capabilities, layout.nodes, layout.ghost)),
    [compact, capabilities, layout],
  );
  const compactEdges = useMemo(
    () =>
      compact
        ? domainEdges(workflows, capabilities).map((edge, index) => ({
            ...edge,
            key: `${edge.from}->${edge.to}`,
            a: layout.hubs[edge.from],
            b: layout.hubs[edge.to],
            path: curve(layout.hubs[edge.from], layout.hubs[edge.to], index, 0.12),
          }))
        : [],
    [compact, workflows, capabilities, layout],
  );

  const [mode, setMode] = useState("workflows");
  const [tourIndex, setTourIndex] = useState(0);
  const [hovered, setHovered] = useState(null);
  const [selected, setSelected] = useState(null);
  const [paused, setPaused] = useState(false);
  const [inView, setInView] = useState(true);

  useEffect(() => {
    const node = layerRef.current;
    if (!node) return undefined;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) setInView(entry.isIntersecting);
      },
      { threshold: 0.05 },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  const focused = hovered ?? selected;
  const activeWorkflows = useMemo(() => {
    if (mode !== "workflows") return [];
    if (focused) {
      return workflows.filter((workflow) => workflow.steps.includes(focused)).map((workflow) => workflow.id);
    }
    const current = workflows[tourIndex % Math.max(workflows.length, 1)];
    return current ? [current.id] : [];
  }, [mode, focused, workflows, tourIndex]);

  const activeNodes = useMemo(() => {
    const set = new Set();
    for (const workflow of workflows) {
      if (!activeWorkflows.includes(workflow.id)) continue;
      for (const step of workflow.steps) set.add(step);
    }
    return set;
  }, [activeWorkflows, workflows]);

  useEffect(() => {
    if (
      mode !== "workflows" ||
      reducedMotion ||
      paused ||
      hovered ||
      selected ||
      !inView
    ) {
      return undefined;
    }
    const timer = setInterval(() => setTourIndex((value) => value + 1), 4800);
    return () => clearInterval(timer);
  }, [compact, mode, reducedMotion, paused, hovered, selected, inView]);

  const selectedCapability = capabilities.find((capability) => capability.id === selected) ?? null;
  const focusedDomain = focused
    ? capabilities.find((capability) => capability.id === focused)?.domain
    : null;

  useEffect(() => {
    if (!selected) return undefined;
    const onKey = (event) => {
      if (event.key === "Escape") setSelected(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selected]);

  function handleSelectDomain(domain) {
    setSelected(null);
    onSelectDomain(domain);
  }

  const requireFocus = focused;
  const edges = compact ? compactEdges : mode === "workflows" ? workflowEdges : requiresEdges;

  return (
    <div className={`hero-map${compact ? " is-compact" : ""}`} ref={layerRef}>
      <svg
        className="map-svg"
        viewBox={`0 0 ${layout.bounds.w} ${layout.bounds.h}`}
        preserveAspectRatio={compact ? "xMidYMid slice" : "xMidYMid meet"}
        role="group"
        aria-label={t.map.ariaLabel}
      >
        <g className="map-edges">
          {edges.map((edge) => {
            const isActive = compact
              ? activeWorkflows.some((id) => edge.workflows.includes(id))
              : mode === "workflows"
                ? edge.workflows.some((id) => activeWorkflows.includes(id))
                : edge.from === requireFocus || edge.to === requireFocus;
            return (
              <path
                key={edge.key}
                d={edge.path}
                className={`map-edge${isActive ? " is-active" : ""}${mode === "requires" ? " is-dashed" : ""}`}
              />
            );
          })}
        </g>

        {!reducedMotion && (
          <g className="map-packets">
            {edges.map((edge) => {
              const isActive = compact
                ? activeWorkflows.some((id) => edge.workflows.includes(id))
                : mode === "workflows" && edge.workflows.some((id) => activeWorkflows.includes(id));
              if (!isActive) return null;
              return (
                <circle key={`packet-${edge.key}`} r={compact ? 3.4 : 2.8} className="map-packet">
                  <animateMotion
                    dur={`${2.2 + (hash(edge.key) % 5) * 0.18}s`}
                    repeatCount="indefinite"
                    path={edge.path}
                  />
                </circle>
              );
            })}
          </g>
        )}

        <g className="map-hubs">
          {DOMAIN_ORDER.map((domain) => {
            const hub = layout.hubs[domain];
            if (!hub) return null;
            const label = t.domains[domain]?.label ?? domain;
            const color = DOMAIN_COLOR[domain];
            const interactive = true;
            const behindText = !compact && hub.x > 280 && hub.x < 920 && hub.y > 100 && hub.y < 620;
            return (
              <g
                key={domain}
                className={`map-hub${interactive ? " is-interactive" : ""}`}
                transform={`translate(${hub.x} ${hub.y})`}
                role={interactive ? "button" : undefined}
                tabIndex={interactive ? 0 : undefined}
                aria-label={interactive ? `${label} ${hub.count}` : undefined}
                onClick={interactive ? () => handleSelectDomain(domain) : undefined}
                onKeyDown={
                  interactive
                    ? (event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          handleSelectDomain(domain);
                        }
                      }
                    : undefined
                }
              >
                {!behindText && (
                  <>
                    <circle className="map-hub-ring" r={compact ? 17 : 13} stroke={color} />
                    <circle className="map-hub-core" r={compact ? 4.5 : 3.4} fill={color} />
                  </>
                )}
                {!compact &&
                  focusedDomain !== domain &&
                  ((hub.x < 270 && hub.y - hub.ring - 14 > 60) ||
                    (hub.x > 930 && hub.y - hub.ring - 14 > 200)) && (
                    <text
                      className="map-hub-label"
                      y={-(hub.ring + 14)}
                      textAnchor="middle"
                    >
                      {label} {hub.count}
                    </text>
                  )}
              </g>
            );
          })}
        </g>

        {!compact &&
          capabilities.map((capability) => {
            const point = layout.nodes[capability.id];
            if (!point) return null;
            const color = DOMAIN_COLOR[capability.domain] ?? "#8b98a9";
            const isFocused = focused === capability.id;
            const isActive = activeNodes.has(capability.id);
            const dimmed = mode === "workflows" && activeWorkflows.length > 0 && !isActive && !isFocused;
            const hub = layout.hubs[capability.domain];
            const isCentral = !compact && hub && hub.x > 280 && hub.x < 920 && hub.y > 100 && hub.y < 620;
            return (
              <g
                key={capability.id}
                className={`map-node${isFocused ? " is-focused" : ""}${dimmed ? " is-dimmed" : ""}${isCentral ? " is-central" : ""}`}
                transform={`translate(${point.x} ${point.y})`}
                role="button"
                tabIndex={0}
                aria-label={`${capability.id}. ${capability.summary}`}
                onPointerEnter={() => setHovered(capability.id)}
                onPointerLeave={() => setHovered(null)}
                onFocus={() => setHovered(capability.id)}
                onBlur={() => setHovered(null)}
                onClick={() => setSelected(capability.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    setSelected(capability.id);
                  }
                }}
              >
                <circle className="map-node-hit" r={16} fill="transparent" />
                <circle className="map-node-glow" r={isFocused ? 12 : 8} fill={color} />
                <circle className="map-node-dot" r={isFocused ? 5.8 : 4.6} fill={color} />
                {isFocused && (
                  <text className="map-node-label" y={-14} textAnchor="middle">
                    {capability.id}
                  </text>
                )}
              </g>
            );
          })}

        {!compact && mode === "requires" && layout.ghost && (
          <g className="map-node is-ghost" transform={`translate(${layout.ghost.x} ${layout.ghost.y})`}>
            <circle className="map-node-hit" r={15} fill="transparent" />
            <circle className="map-node-dot" r={4} />
            <text className="map-node-label" x={-10} y={4} textAnchor="end">
              {t.map.session}
            </text>
          </g>
        )}
      </svg>

      {!compact && (
        <div
          className="map-toolbar"
          onPointerEnter={() => setPaused(true)}
          onPointerLeave={() => setPaused(false)}
        >
          <div className="map-modes" role="group" aria-label={t.map.ariaLabel}>
            <button
              type="button"
              className={mode === "workflows" ? "is-active" : ""}
              onClick={() => setMode("workflows")}
            >
              {t.map.workflows}
            </button>
            <button
              type="button"
              className={mode === "requires" ? "is-active" : ""}
              onClick={() => setMode("requires")}
            >
              {t.map.requires}
            </button>
          </div>
          {mode === "workflows" && (
            <div className="map-flow-list">
              {workflows.map((workflow, index) => (
                <button
                  key={workflow.id}
                  type="button"
                  className={
                    index === tourIndex % Math.max(workflows.length, 1) && !focused
                      ? "is-active"
                      : ""
                  }
                  title={workflow.id}
                  onClick={() => setTourIndex(index)}
                >
                  {workflow.id.split("-")[0]}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {!compact && <p className="map-hint">{t.map.hint}</p>}

      {selectedCapability && (
        <CapabilityDetail
          t={t}
          capability={selectedCapability}
          onClose={() => setSelected(null)}
          onSelectDomain={handleSelectDomain}
        />
      )}
    </div>
  );
}
