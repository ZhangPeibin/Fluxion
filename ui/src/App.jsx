import { useEffect, useMemo, useRef, useState } from "react";
import { Network } from "vis-network/standalone";

import { PLATFORM_COLORS, PLATFORM_LABELS, buildGraphData } from "./graph.js";

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

export default function App() {
  const containerRef = useRef(null);
  const [graph, setGraph] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const controller = new AbortController();

    async function load() {
      try {
        const response = await fetch("/api/graph", { signal: controller.signal });
        if (!response.ok) {
          throw new Error(`GET /api/graph → ${response.status} ${response.statusText}`);
        }
        setGraph(await response.json());
        setError(null);
      } catch (err) {
        if (err.name === "AbortError") {
          return;
        }
        setError(err.message);
      } finally {
        if (!controller.signal.aborted) {
          setLoading(false);
        }
      }
    }

    load();
    return () => controller.abort();
  }, []);

  const { nodes, edges } = useMemo(() => buildGraphData(graph), [graph]);

  useEffect(() => {
    if (!containerRef.current || !graph) {
      return undefined;
    }
    const network = new Network(containerRef.current, { nodes, edges }, NETWORK_OPTIONS);
    return () => network.destroy();
  }, [graph, nodes, edges]);

  const tokenCount = graph?.tokens?.length ?? 0;
  const edgeCount = graph?.edges?.length ?? 0;

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Fluxion graph</h1>
        <p className="subtitle">Read-only view of the routing multigraph.</p>
        {loading && <p className="status">Loading graph…</p>}
        {error && <p className="error">{error}</p>}
        {!loading && !error && tokenCount === 0 && (
          <p className="status">The service returned an empty graph.</p>
        )}
        {!loading && !error && tokenCount > 0 && (
          <p className="status">
            {tokenCount} tokens · {edgeCount} directed edges
          </p>
        )}
        <div className="legend">
          <h2>Platforms</h2>
          {Object.entries(PLATFORM_COLORS).map(([platform, color]) => (
            <div className="legend-item" key={platform}>
              <span className="swatch" style={{ background: color }} />
              <span>{PLATFORM_LABELS[platform] ?? platform}</span>
            </div>
          ))}
        </div>
      </aside>
      <div className="canvas" ref={containerRef} />
    </div>
  );
}
