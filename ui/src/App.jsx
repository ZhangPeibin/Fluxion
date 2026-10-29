import { useEffect, useMemo, useRef, useState } from "react";
import { DataSet, Network } from "vis-network/standalone";

import { PLATFORM_COLORS, PLATFORM_LABELS, edgeKey, shortAddress, syncGraphData } from "./graph.js";

const DEFAULT_REFRESH_SECONDS = 5;
const ALL_PLATFORMS = Object.keys(PLATFORM_COLORS);
const INITIAL_ENABLED = Object.fromEntries(ALL_PLATFORMS.map((platform) => [platform, true]));

const ADDRESS_PATTERN = /^0x[0-9a-fA-F]{40}$/;
const AMOUNT_PATTERN = /^\d+$/;

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

function GraphCanvas({ graph, enabledPlatforms, tokenIn, tokenOut, routeEdgeIds, onNodeClick }) {
  const containerRef = useRef(null);
  const nodesRef = useRef(null);
  const edgesRef = useRef(null);
  const onNodeClickRef = useRef(onNodeClick);

  if (!nodesRef.current) {
    nodesRef.current = new DataSet();
  }
  if (!edgesRef.current) {
    edgesRef.current = new DataSet();
  }

  useEffect(() => {
    onNodeClickRef.current = onNodeClick;
  });

  useEffect(() => {
    const network = new Network(
      containerRef.current,
      { nodes: nodesRef.current, edges: edgesRef.current },
      NETWORK_OPTIONS,
    );
    network.on("click", (params) => {
      if (params.nodes.length === 1) {
        onNodeClickRef.current?.(params.nodes[0]);
      }
    });
    // The topology is static at runtime; freezing physics after the initial
    // layout keeps node positions (and the camera) stable across refreshes.
    network.once("stabilizationIterationsDone", () => {
      network.setOptions({ physics: { enabled: false } });
    });
    return () => network.destroy();
  }, []);

  useEffect(() => {
    syncGraphData(nodesRef.current, edgesRef.current, graph, {
      enabledPlatforms,
      tokenIn,
      tokenOut,
      routeEdgeIds,
    });
  }, [graph, enabledPlatforms, tokenIn, tokenOut, routeEdgeIds]);

  return <div className="canvas" ref={containerRef} />;
}

export default function App() {
  const [graph, setGraph] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);
  const [refreshSeconds, setRefreshSeconds] = useState(DEFAULT_REFRESH_SECONDS);
  const [enabled, setEnabled] = useState(INITIAL_ENABLED);

  const [tokenIn, setTokenIn] = useState("");
  const [tokenOut, setTokenOut] = useState("");
  const [activeField, setActiveField] = useState("in");
  const [amount, setAmount] = useState("1");
  const [quote, setQuote] = useState(null);
  const [quoteError, setQuoteError] = useState(null);
  const [quoting, setQuoting] = useState(false);

  const enabledPlatforms = useMemo(
    () => new Set(ALL_PLATFORMS.filter((platform) => enabled[platform])),
    [enabled],
  );

  const routeEdgeIds = useMemo(
    () => new Set((quote?.path ?? []).map((hop) => edgeKey(hop.pool, hop.i, hop.j))),
    [quote],
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

  const trimmedIn = tokenIn.trim();
  const trimmedOut = tokenOut.trim();
  const trimmedAmount = amount.trim();

  useEffect(() => {
    if (
      !ADDRESS_PATTERN.test(trimmedIn) ||
      !ADDRESS_PATTERN.test(trimmedOut) ||
      !AMOUNT_PATTERN.test(trimmedAmount)
    ) {
      setQuote(null);
      setQuoteError(null);
      setQuoting(false);
      return undefined;
    }

    const controller = new AbortController();
    const timer = setTimeout(async () => {
      setQuoting(true);
      try {
        const response = await fetch("/api/quote", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            token_in: trimmedIn,
            token_out: trimmedOut,
            amount_in: trimmedAmount,
          }),
          signal: controller.signal,
        });
        const payload = await response.json().catch(() => null);
        if (!response.ok) {
          const detail =
            payload?.message ?? payload?.error ?? `${response.status} ${response.statusText}`;
          throw new Error(detail);
        }
        setQuote(payload);
        setQuoteError(null);
      } catch (err) {
        if (err.name === "AbortError") {
          return;
        }
        setQuote(null);
        setQuoteError(err.message);
      } finally {
        if (!controller.signal.aborted) {
          setQuoting(false);
        }
      }
    }, 300);

    return () => {
      clearTimeout(timer);
      controller.abort();
    };
  }, [trimmedIn, trimmedOut, trimmedAmount]);

  function togglePlatform(platform) {
    setEnabled((current) => ({ ...current, [platform]: !current[platform] }));
  }

  function updateRefreshSeconds(event) {
    const next = Number(event.target.value);
    if (Number.isInteger(next) && next >= 1) {
      setRefreshSeconds(Math.min(next, 3600));
    }
  }

  function handleNodeClick(token) {
    if (activeField === "in") {
      setTokenIn(token);
      if (!tokenOut) {
        setActiveField("out");
      }
    } else {
      setTokenOut(token);
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

        <div className="route">
          <h2>Route</h2>
          <label className="field">
            <span>From</span>
            <input
              className="route-input"
              value={tokenIn}
              onChange={(event) => setTokenIn(event.target.value)}
              onFocus={() => setActiveField("in")}
              placeholder="0x… token address"
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span>To</span>
            <input
              className="route-input"
              value={tokenOut}
              onChange={(event) => setTokenOut(event.target.value)}
              onFocus={() => setActiveField("out")}
              placeholder="0x… token address"
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span>Amount (raw units)</span>
            <input
              className="route-input"
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
              placeholder="1"
              inputMode="numeric"
              spellCheck={false}
            />
          </label>
          <p className="hint">Click a node to fill the focused field.</p>

          {quoting && <p className="status">Quoting…</p>}
          {quoteError && <p className="error">Quote failed: {quoteError}</p>}
          {quote && (
            <div className="route-result">
              <dl className="quote-metrics">
                <div>
                  <dt>Amount out</dt>
                  <dd>{quote.amount_out}</dd>
                </div>
                <div>
                  <dt>Price impact</dt>
                  <dd>{quote.price_impact_bps} bps</dd>
                </div>
              </dl>
              {quote.path.length === 0 ? (
                <p className="status">Input and output are the same token.</p>
              ) : (
                <ol className="hops">
                  {quote.path.map((hop) => (
                    <li key={edgeKey(hop.pool, hop.i, hop.j)}>
                      <span className="hop-tokens">
                        {shortAddress(hop.token_in)} → {shortAddress(hop.token_out)}
                      </span>
                      <span className="hop-meta">
                        {hop.platform} · {shortAddress(hop.pool)}
                      </span>
                    </li>
                  ))}
                </ol>
              )}
            </div>
          )}
        </div>

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
      <GraphCanvas
        graph={graph}
        enabledPlatforms={enabledPlatforms}
        tokenIn={tokenIn}
        tokenOut={tokenOut}
        routeEdgeIds={routeEdgeIds}
        onNodeClick={handleNodeClick}
      />
    </div>
  );
}
