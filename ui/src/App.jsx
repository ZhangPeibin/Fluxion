import { useEffect, useMemo, useRef, useState } from "react";
import { DataSet, Network } from "vis-network/standalone";

import {
  PLATFORM_COLORS,
  PLATFORM_LABELS,
  defaultAmountForToken,
  edgeKey,
  shortAddress,
  syncGraphData,
  tokenLabel,
  tokenSymbol,
} from "./graph.js";

const DEFAULT_REFRESH_SECONDS = 5;
const ALL_PLATFORMS = Object.keys(PLATFORM_COLORS);
const INITIAL_ENABLED = Object.fromEntries(ALL_PLATFORMS.map((platform) => [platform, true]));

const ADDRESS_PATTERN = /^0x[0-9a-fA-F]{40}$/;
const AMOUNT_PATTERN = /^\d+$/;
const ETHERSCAN_ADDRESS = "https://etherscan.io/address/";
const QUOTE_ERROR_MESSAGES = {
  NoPathFound: "NoPathFound — no route for this amount; try a larger amount or other tokens",
};

const NETWORK_OPTIONS = {
  nodes: {
    shape: "dot",
    size: 11,
    color: { background: "#58a6ff", border: "#1f6feb" },
    font: { size: 13, color: "#e6edf3" },
  },
  edges: {
    arrows: { to: { enabled: true, scaleFactor: 0.6 } },
    width: 1,
    font: { size: 8, strokeWidth: 0 },
  },
  physics: {
    stabilization: { iterations: 200 },
    barnesHut: { gravitationalConstant: -12000, springLength: 140 },
  },
  interaction: { hover: true, tooltipDelay: 120 },
};

function GraphCanvas({
  graph,
  enabledPlatforms,
  tokenIn,
  tokenOut,
  routeEdgeIds,
  showEdgeLabels,
  searchQuery,
  minLiquidity,
  onNodeClick,
  onEdgeClick,
}) {
  const containerRef = useRef(null);
  const nodesRef = useRef(null);
  const edgesRef = useRef(null);
  const onNodeClickRef = useRef(onNodeClick);
  const onEdgeClickRef = useRef(onEdgeClick);

  if (!nodesRef.current) {
    nodesRef.current = new DataSet();
  }
  if (!edgesRef.current) {
    edgesRef.current = new DataSet();
  }

  useEffect(() => {
    onNodeClickRef.current = onNodeClick;
    onEdgeClickRef.current = onEdgeClick;
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
      } else if (params.edges.length === 1) {
        onEdgeClickRef.current?.(params.edges[0]);
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
      showEdgeLabels,
      searchQuery,
      minLiquidity,
    });
  }, [
    graph,
    enabledPlatforms,
    tokenIn,
    tokenOut,
    routeEdgeIds,
    showEdgeLabels,
    searchQuery,
    minLiquidity,
  ]);

  return <div className="canvas" ref={containerRef} />;
}

export default function App() {
  const [graph, setGraph] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);
  const [refreshSeconds, setRefreshSeconds] = useState(DEFAULT_REFRESH_SECONDS);
  const [enabled, setEnabled] = useState(INITIAL_ENABLED);
  const [showEdgeLabels, setShowEdgeLabels] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [minLiquidity, setMinLiquidity] = useState("");

  const [tokenIn, setTokenIn] = useState("");
  const [tokenOut, setTokenOut] = useState("");
  const [activeField, setActiveField] = useState("in");
  const [amount, setAmount] = useState("1");
  const [amountTouched, setAmountTouched] = useState(false);
  const [quote, setQuote] = useState(null);
  const [quoteError, setQuoteError] = useState(null);
  const [quoting, setQuoting] = useState(false);
  const [details, setDetails] = useState(null);

  const enabledPlatforms = useMemo(
    () => new Set(ALL_PLATFORMS.filter((platform) => enabled[platform])),
    [enabled],
  );

  const platformCounts = useMemo(() => {
    const counts = Object.fromEntries(ALL_PLATFORMS.map((platform) => [platform, 0]));
    for (const edge of graph?.edges ?? []) {
      counts[edge.platform] = (counts[edge.platform] ?? 0) + 1;
    }
    return counts;
  }, [graph]);

  const edgesById = useMemo(() => {
    const index = new Map();
    for (const edge of graph?.edges ?? []) {
      index.set(edgeKey(edge.pool, edge.i, edge.j), edge);
    }
    return index;
  }, [graph]);

  const nodeDegrees = useMemo(() => {
    const degrees = new Map();
    for (const edge of graph?.edges ?? []) {
      degrees.set(edge.from, (degrees.get(edge.from) ?? 0) + 1);
      degrees.set(edge.to, (degrees.get(edge.to) ?? 0) + 1);
    }
    return degrees;
  }, [graph]);

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
          throw new Error(QUOTE_ERROR_MESSAGES[detail] ?? detail);
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

  function updateTokenIn(value) {
    setTokenIn(value);
    if (!amountTouched) {
      setAmount(defaultAmountForToken(value));
    }
  }

  function handleNodeClick(token) {
    setDetails({ kind: "node", address: token });
    if (activeField === "in") {
      updateTokenIn(token);
      if (!tokenOut) {
        setActiveField("out");
      }
    } else {
      setTokenOut(token);
    }
  }

  function handleEdgeClick(edgeId) {
    setDetails({ kind: "edge", id: edgeId });
  }

  const tokenCount = graph?.tokens?.length ?? 0;
  const edgeCount = graph?.edges?.length ?? 0;
  const selectedEdge = details?.kind === "edge" ? edgesById.get(details.id) : null;

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
              onChange={(event) => updateTokenIn(event.target.value)}
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
              onChange={(event) => {
                setAmount(event.target.value);
                setAmountTouched(true);
              }}
              placeholder="1"
              inputMode="numeric"
              spellCheck={false}
            />
          </label>
          <p className="hint">
            Click a node to fill the focused field. Amount defaults to one whole token in raw units
            (e.g. 1e18 for an 18-decimal token).
          </p>

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
                        {tokenLabel(hop.token_in)} → {tokenLabel(hop.token_out)}
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

        <div className="filters">
          <h2>Filters</h2>
          <label className="field">
            <span>Token search</span>
            <input
              className="route-input"
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="symbol or 0x… address"
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span>Min liquidity (raw reserve)</span>
            <input
              className="route-input"
              value={minLiquidity}
              onChange={(event) => setMinLiquidity(event.target.value)}
              placeholder="0 = show all"
              inputMode="numeric"
              spellCheck={false}
            />
          </label>
          <p className="hint">
            Search dims tokens that do not match. Min reserve hides thinner V2/Curve edges (V3
            reports max reserve).
          </p>
        </div>

        {details && (
          <div className="details">
            <div className="details-header">
              <h2>{details.kind === "node" ? "Token details" : "Edge details"}</h2>
              <button
                type="button"
                className="close"
                onClick={() => setDetails(null)}
                aria-label="Close details"
              >
                ×
              </button>
            </div>
            {details.kind === "node" ? (
              <dl className="details-list">
                <div>
                  <dt>Token</dt>
                  <dd>{tokenSymbol(details.address) ?? "Unknown token"}</dd>
                </div>
                <div>
                  <dt>Address</dt>
                  <dd className="mono">{details.address}</dd>
                </div>
                <div>
                  <dt>Directed edges</dt>
                  <dd>{nodeDegrees.get(details.address) ?? 0}</dd>
                </div>
                <div>
                  <dt>Explorer</dt>
                  <dd>
                    <a
                      href={`${ETHERSCAN_ADDRESS}${details.address}`}
                      target="_blank"
                      rel="noreferrer"
                    >
                      Etherscan
                    </a>
                  </dd>
                </div>
              </dl>
            ) : selectedEdge ? (
              <dl className="details-list">
                <div>
                  <dt>From</dt>
                  <dd className="mono">
                    {tokenLabel(selectedEdge.from)} · {selectedEdge.from}
                  </dd>
                </div>
                <div>
                  <dt>To</dt>
                  <dd className="mono">
                    {tokenLabel(selectedEdge.to)} · {selectedEdge.to}
                  </dd>
                </div>
                <div>
                  <dt>Platform</dt>
                  <dd>{selectedEdge.platform}</dd>
                </div>
                <div>
                  <dt>Pool</dt>
                  <dd className="mono">{selectedEdge.pool}</dd>
                </div>
                <div>
                  <dt>Coin indices</dt>
                  <dd>
                    {selectedEdge.i} → {selectedEdge.j}
                  </dd>
                </div>
                <div>
                  <dt>Mid price</dt>
                  <dd>{selectedEdge.mid_price ?? "—"}</dd>
                </div>
                <div>
                  <dt>Reserve</dt>
                  <dd className="mono">{selectedEdge.reserve ?? "—"}</dd>
                </div>
                <div>
                  <dt>Explorer</dt>
                  <dd>
                    <a
                      href={`${ETHERSCAN_ADDRESS}${selectedEdge.pool}`}
                      target="_blank"
                      rel="noreferrer"
                    >
                      Etherscan
                    </a>
                  </dd>
                </div>
              </dl>
            ) : (
              <p className="status">This edge is no longer in the graph.</p>
            )}
          </div>
        )}

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
              <span className="legend-count">{platformCounts[platform] ?? 0}</span>
            </label>
          ))}
          <label className="legend-item">
            <input
              type="checkbox"
              checked={showEdgeLabels}
              onChange={(event) => setShowEdgeLabels(event.target.checked)}
            />
            <span>Edge labels</span>
          </label>
        </div>
      </aside>
      <GraphCanvas
        graph={graph}
        enabledPlatforms={enabledPlatforms}
        tokenIn={tokenIn}
        tokenOut={tokenOut}
        routeEdgeIds={routeEdgeIds}
        showEdgeLabels={showEdgeLabels}
        searchQuery={searchQuery}
        minLiquidity={minLiquidity}
        onNodeClick={handleNodeClick}
        onEdgeClick={handleEdgeClick}
      />
    </div>
  );
}
