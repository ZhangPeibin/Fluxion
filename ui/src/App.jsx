import { useEffect, useMemo, useRef, useState } from "react";
import { DataSet, Network } from "vis-network/standalone";

import { PLATFORM_COLORS, PLATFORM_LABELS, syncGraphData } from "./graph.js";

const DEFAULT_REFRESH_SECONDS = 5;
const ALL_PLATFORMS = Object.keys(PLATFORM_COLORS);
const INITIAL_ENABLED = Object.fromEntries(ALL_PLATFORMS.map((platform) => [platform, true]));

const NETWORK_OPTIONS = {
  nodes: {
    shape: "dot",
    size: 10,
    color: { background: "#58a6ff", border: "#1f6feb" },
    font: { size: 12, color: "#e6edf3" },
  },
  edges: {
    arrows: { to: { enabled: true, scaleFactor: 0.6 } },
    width: 1.2,
    font: { size: 9, strokeWidth: 0 },
  },
  physics: {
    stabilization: { iterations: 200 },
    barnesHut: { gravitationalConstant: -12000, springLength: 140 },
  },
  interaction: { hover: true, tooltipDelay: 120 },
};

function GraphCanvas({ graph, enabledPlatforms }) {
  const containerRef = useRef(null);
  const nodesRef = useRef(null);
  const edgesRef = useRef(null);

  if (!nodesRef.current) {
    nodesRef.current = new DataSet();
  }
  if (!edgesRef.current) {
    edgesRef.current = new DataSet();
  }

  useEffect(() => {
    const network = new Network(
      containerRef.current,
      { nodes: nodesRef.current, edges: edgesRef.current },
      NETWORK_OPTIONS,
    );
    // The topology is static at runtime; freezing physics after the initial
    // layout keeps node positions (and the camera) stable across refreshes.
    network.once("stabilizationIterationsDone", () => {
      network.setOptions({ physics: { enabled: false } });
    });
    return () => network.destroy();
  }, []);

  useEffect(() => {
    syncGraphData(nodesRef.current, edgesRef.current, graph, enabledPlatforms);
  }, [graph, enabledPlatforms]);

  return <div className="canvas" ref={containerRef} />;
}

export default function App() {
  const [graph, setGraph] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);
  const [refreshSeconds, setRefreshSeconds] = useState(DEFAULT_REFRESH_SECONDS);
  const [enabled, setEnabled] = useState(INITIAL_ENABLED);

  const enabledPlatforms = useMemo(
    () => new Set(ALL_PLATFORMS.filter((platform) => enabled[platform])),
    [enabled],
  );

  useEffect(() => {
    let disposed = false;
    const controller = new AbortController();

    async function load() {
      try {
        const response = await fetch("/api/graph", {
          cache: "no-store",
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(`GET /api/graph → ${response.status} ${response.statusText}`);
        }
        const data = await response.json();
        if (disposed) {
          return;
        }
        setGraph(data);
        setError(null);
      } catch (err) {
        if (err.name === "AbortError" || disposed) {
          return;
        }
        setError(err.message);
      } finally {
        if (!disposed) {
          setLoading(false);
        }
      }
    }

    load();
    const timer = setInterval(load, refreshSeconds * 1000);
    return () => {
      disposed = true;
      clearInterval(timer);
      controller.abort();
    };
  }, [refreshSeconds]);

  function togglePlatform(platform) {
    setEnabled((current) => ({ ...current, [platform]: !current[platform] }));
  }

  function updateRefreshSeconds(event) {
    const next = Number(event.target.value);
    if (Number.isInteger(next) && next >= 1) {
      setRefreshSeconds(Math.min(next, 3600));
    }
  }

  const tokenCount = graph?.tokens?.length ?? 0;
  const edgeCount = graph?.edges?.length ?? 0;

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Fluxion graph</h1>
        <p className="subtitle">Read-only view of the routing multigraph.</p>
        {loading && <p className="status">Loading graph…</p>}
        {error && (
          <p className="error">
            {graph ? "Refresh failed: " : ""}
            {error}
          </p>
        )}
        {!loading && !error && tokenCount === 0 && (
          <p className="status">The service returned an empty graph.</p>
        )}
        {!loading && !error && tokenCount > 0 && (
          <p className="status">
            {tokenCount} tokens · {edgeCount} directed edges
          </p>
        )}

        <label className="control">
          <span>Refresh every</span>
          <span className="control-row">
            <input
              type="number"
              min="1"
              max="3600"
              step="1"
              value={refreshSeconds}
              onChange={updateRefreshSeconds}
            />
            <span>seconds</span>
          </span>
        </label>

        <div className="legend">
          <h2>Platforms</h2>
          {ALL_PLATFORMS.map((platform) => (
            <label className="legend-item" key={platform}>
              <input
                type="checkbox"
                checked={Boolean(enabled[platform])}
                onChange={() => togglePlatform(platform)}
              />
              <span className="swatch" style={{ background: PLATFORM_COLORS[platform] }} />
              <span>{PLATFORM_LABELS[platform] ?? platform}</span>
            </label>
          ))}
        </div>
      </aside>
      <GraphCanvas graph={graph} enabledPlatforms={enabledPlatforms} />
    </div>
  );
}
