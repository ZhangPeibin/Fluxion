import { DataSet } from "vis-network/standalone";

export const PLATFORM_COLORS = {
  "uniswap-v2": "#3fb950",
  "uniswap-v3": "#d29922",
  curve: "#a371f7",
};

export const PLATFORM_LABELS = {
  "uniswap-v2": "Uniswap V2",
  "uniswap-v3": "Uniswap V3",
  curve: "Curve",
};

const FALLBACK_COLOR = "#8b949e";

const DEFAULT_NODE_COLOR = { background: "#58a6ff", border: "#1f6feb" };
const SOURCE_NODE_COLOR = { background: "#2ea043", border: "#3fb950" };
const TARGET_NODE_COLOR = { background: "#da3633", border: "#f85149" };

export function shortAddress(address) {
  if (typeof address !== "string" || address.length < 12) {
    return String(address);
  }
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
}

function edgeId(edge) {
  return edgeKey(edge.pool, edge.i, edge.j);
}

/**
 * Stable identity for a directed pool hop, shared by graph edges and quote
 * hops so the best route can be matched back onto the drawn edges.
 */
export function edgeKey(pool, i, j) {
  return `${String(pool).toLowerCase()}:${i}:${j}`;
}

function edgeTooltip(edge) {
  return [
    `pool ${edge.pool}`,
    `platform ${edge.platform}`,
    edge.mid_price === undefined ? null : `mid price ${edge.mid_price}`,
    edge.reserve === undefined ? null : `reserve ${edge.reserve}`,
  ]
    .filter(Boolean)
    .join("\n");
}

/**
 * Map liquidity (input reserve, as a decimal string) onto an edge width.
 * Log-scaled and clamped so deep pools stand out without dominating the canvas.
 */
export function liquidityWidth(reserve) {
  const value = Number(reserve);
  if (!Number.isFinite(value) || value <= 0) {
    return 1;
  }
  const width = 0.8 + Math.log10(value + 1) * 0.25;
  return Math.min(4, Math.max(0.8, width));
}

function hexToRgba(hex, alpha) {
  const value = hex.replace("#", "");
  const r = Number.parseInt(value.slice(0, 2), 16);
  const g = Number.parseInt(value.slice(2, 4), 16);
  const b = Number.parseInt(value.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function validPrice(edge) {
  return typeof edge.mid_price === "number" &&
    Number.isFinite(edge.mid_price) &&
    edge.mid_price > 0
    ? edge.mid_price
    : null;
}

/**
 * Reconcile the service's `/graph` payload into existing vis-network datasets.
 *
 * Updates are in place (stable ids per token and per pool hop) so a refresh
 * reuses node positions, camera state and any current selection instead of
 * rebuilding the network. Edges keep the platform colour and are weighted by
 * liquidity (width); opacity encodes mid-price competitiveness within the same
 * direction. Edges are hidden when their platform is filtered out.
 *
 * When `routeEdgeIds` is a non-empty set (the best route from `/quote`), those
 * hops are emphasised and every other edge is dimmed. Source/target tokens are
 * recoloured to show the selection.
 */
export function syncGraphData(nodes, edges, graph, options = {}) {
  const { enabledPlatforms, tokenIn, tokenOut, routeEdgeIds } = options;
  if (!graph) {
    return;
  }

  const tokenIds = new Set(graph.tokens ?? []);
  const staleNodes = nodes.getIds().filter((id) => !tokenIds.has(id));
  if (staleNodes.length > 0) {
    nodes.remove(staleNodes);
  }
  const sourceId = typeof tokenIn === "string" ? tokenIn.toLowerCase() : null;
  const targetId = typeof tokenOut === "string" ? tokenOut.toLowerCase() : null;
  for (const token of tokenIds) {
    const tokenId = token.toLowerCase();
    let color = DEFAULT_NODE_COLOR;
    if (sourceId && tokenId === sourceId) {
      color = SOURCE_NODE_COLOR;
    } else if (targetId && tokenId === targetId) {
      color = TARGET_NODE_COLOR;
    }
    nodes.update({ id: token, label: shortAddress(token), title: token, color });
  }

  const edgeIds = new Set();
  const parallelCount = new Map();
  const bestPrices = new Map();
  for (const edge of graph.edges ?? []) {
    const price = validPrice(edge);
    if (price === null) {
      continue;
    }
    const key = `${edge.from}->${edge.to}`;
    bestPrices.set(key, Math.max(bestPrices.get(key) ?? 0, price));
  }

  (graph.edges ?? []).forEach((edge) => {
    const id = edgeId(edge);
    edgeIds.add(id);

    const key = `${edge.from}->${edge.to}`;
    const parallelIndex = parallelCount.get(key) ?? 0;
    parallelCount.set(key, parallelIndex + 1);

    // Colour keeps the platform hue (the filter/legend key); alpha encodes how
    // competitive this pool's mid price is among parallel hops for the same
    // direction (1 = best available).
    const platformColor = PLATFORM_COLORS[edge.platform] ?? FALLBACK_COLOR;
    const price = validPrice(edge);
    const bestPrice = bestPrices.get(key) ?? 0;
    const priceRatio = price !== null && bestPrice > 0 ? Math.min(1, price / bestPrice) : null;
    const reverse = edge.from > edge.to;

    const hasRoute = routeEdgeIds instanceof Set && routeEdgeIds.size > 0;
    const highlighted = hasRoute && routeEdgeIds.has(id);
    const dimmed = hasRoute && !highlighted;

    let width = liquidityWidth(edge.reserve);
    let alpha = priceRatio === null ? 1 : 0.4 + 0.6 * priceRatio;
    let fontColor = platformColor;
    let shadow = { enabled: false };
    if (highlighted) {
      width = Math.max(4, width + 1.5);
      alpha = 1;
      shadow = { enabled: true, color: platformColor, size: 8 };
    } else if (dimmed) {
      width = Math.max(0.5, width * 0.5);
      alpha = 0.12;
      fontColor = "rgba(230, 237, 243, 0.2)";
    }
    const color = hexToRgba(platformColor, alpha);

    edges.update({
      id,
      from: edge.from,
      to: edge.to,
      label: `${edge.platform} ${shortAddress(edge.pool)}`,
      title: edgeTooltip(edge),
      color: { color, highlight: platformColor, hover: platformColor },
      font: { color: fontColor },
      width,
      shadow,
      hidden: enabledPlatforms ? !enabledPlatforms.has(edge.platform) : false,
      smooth: {
        enabled: true,
        type: reverse ? "curvedCCW" : "curvedCW",
        roundness: 0.12 + 0.1 * parallelIndex,
      },
    });
  });

  const staleEdges = edges.getIds().filter((id) => !edgeIds.has(id));
  if (staleEdges.length > 0) {
    edges.remove(staleEdges);
  }
}
