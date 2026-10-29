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

// Symbols for the built-in major-token allowlist (and other common mainnet
// tokens). Unknown tokens fall back to their short address.
const TOKEN_SYMBOLS = {
  "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2": "WETH",
  "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599": "WBTC",
  "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48": "USDC",
  "0xdac17f958d2ee523a2206206994597c13d831ec7": "USDT",
  "0x6b175474e89094c44da98b954eedeac495271d0f": "DAI",
  "0xdc035d45d973e3ec169d2276ddab16f1e407384f": "USDS",
  "0xae7ab96520de3a18e5e111b5eaab095312d7fe84": "stETH",
  "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0": "wstETH",
  "0x514910771af9ca656af840dff83e8264ecf986ca": "LINK",
  "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984": "UNI",
  "0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9": "AAVE",
  "0x9f8f72aa9304c8b593d555f12ef6589cc3a579a2": "MKR",
  "0xd533a949740bb3306d119cc777fa900ba034cd52": "CRV",
  "0x5a98fcbea516cf06857215779fd812ca3bef1b32": "LDO",
};

const TOKEN_DECIMALS = {
  "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599": 8,
  "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48": 6,
  "0xdac17f958d2ee523a2206206994597c13d831ec7": 6,
};

const DEFAULT_NODE_COLOR = { background: "#58a6ff", border: "#1f6feb" };
const SOURCE_NODE_COLOR = { background: "#2ea043", border: "#3fb950" };
const TARGET_NODE_COLOR = { background: "#da3633", border: "#f85149" };

export function tokenSymbol(address) {
  if (typeof address !== "string") {
    return null;
  }
  return TOKEN_SYMBOLS[address.toLowerCase()] ?? null;
}

export function tokenDecimals(address) {
  if (typeof address !== "string") {
    return 18;
  }
  return TOKEN_DECIMALS[address.toLowerCase()] ?? 18;
}

/** Raw-unit amount for one whole token (the UI's default input). */
export function defaultAmountForToken(address) {
  if (typeof address !== "string" || address.trim() === "") {
    return "";
  }
  return (10n ** BigInt(tokenDecimals(address))).toString();
}

export function tokenLabel(address) {
  return tokenSymbol(address) ?? shortAddress(address);
}

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
    `${tokenLabel(edge.from)} → ${tokenLabel(edge.to)}`,
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
    return 0.8;
  }
  const width = 0.6 + Math.log10(value + 1) * 0.16;
  return Math.min(2, Math.max(0.6, width));
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
  const { enabledPlatforms, tokenIn, tokenOut, routeEdgeIds, showEdgeLabels } = options;
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
  const degrees = new Map();
  for (const edge of graph.edges ?? []) {
    degrees.set(edge.from, (degrees.get(edge.from) ?? 0) + 1);
    degrees.set(edge.to, (degrees.get(edge.to) ?? 0) + 1);
  }
  for (const token of tokenIds) {
    const tokenId = token.toLowerCase();
    let color = DEFAULT_NODE_COLOR;
    if (sourceId && tokenId === sourceId) {
      color = SOURCE_NODE_COLOR;
    } else if (targetId && tokenId === targetId) {
      color = TARGET_NODE_COLOR;
    }
    const symbol = tokenSymbol(token);
    nodes.update({
      id: token,
      label: symbol ?? shortAddress(token),
      title: `${symbol ? `${symbol} (${token})` : token}\n${degrees.get(token) ?? 0} directed edges`,
      color,
    });
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
      width = Math.max(2.6, width + 0.4);
      alpha = 1;
      shadow = { enabled: true, color: platformColor, size: 5 };
    } else if (dimmed) {
      width = Math.max(0.4, width * 0.5);
      alpha = 0.12;
      fontColor = "rgba(230, 237, 243, 0.2)";
    }
    const color = hexToRgba(platformColor, alpha);

    edges.update({
      id,
      from: edge.from,
      to: edge.to,
      label: showEdgeLabels ? `${edge.platform} ${shortAddress(edge.pool)}` : "",
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
