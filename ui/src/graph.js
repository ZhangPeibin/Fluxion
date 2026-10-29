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
  "0x95ad61b0a150d79219dcf64e1e6cc01f0b64c4ce": "SHIB",
  "0x6982508145454ce325ddbe47a25d4ec3d2311933": "PEPE",
  "0xaea46a60368a7bd060eec7df8cba43b7ef41ad85": "FET",
  "0x6de037ef9ad2725eb40118bb1702ebb27e4aeb24": "RNDR",
  "0xc944e90c64b2c07662a292be6244bdf05cda44a7": "GRT",
  "0x4d224452801aced8b2f0aebe155379bb5d594381": "APE",
  "0xb50721bcf8d664c30412cfbc6cf7a15145234ad1": "ARB",
  "0xf57e7e7c23978c3caec3c3548e3d615c346e79ff": "IMX",
  "0xe28b3b32b6c345a34ff64674606124dd5aceca30": "INJ",
  "0xc18360217d8f7ab5e7c516566761ea12ce7f9d72": "ENS",
  "0xc00e94cb662c3520282e6f5717214004a7f26888": "COMP",
  "0xc011a73ee8576fb46f5e1c5751ca3b9fe0af2a6f": "SNX",
  "0xba100000625a3754423978a60c9317c58a424e3d": "BAL",
  "0x111111111117dc0aa78b770fa6a738034120c302": "1INCH",
  "0x3432b6a60d23ca0dfca7761b7ab56459d9c964d0": "FXS",
  "0x92d6c1e31e14520e676a687f0a93788b716beff5": "DYDX",
  "0x5283d291dbcf85356a21ba090e6db59121208b44": "BLUR",
  "0x6dea81c8171d0ba574754ef6f8b412f2ed88c54d": "LQTY",
  "0xca14007eff0db1f8135f4c25b34de49ab0d42766": "STRK",
  "0x967da4048cd07ab37855c090aaf366e4ce1b9f48": "OCEAN",
  "0x69af81e73a73b40adf4f3d4223cd9b1ece623074": "MASK",
  "0x152649ea73beab28c5b49b26eb48f7ead6d4c898": "CAKE",
  "0x6b3595068778dd592e39a122f4f5a5cf09c90fe2": "SUSHI",
  "0x0bc529c00c6401aef6d220be8c6ea1667f6ad93e": "YFI",
  "0xe41d2489571d322189246dafa5ebde1f4699f498": "ZRX",
  "0x0d8775f648430679a709e98d2b0cb6250d2887ef": "BAT",
  "0x3506424f91fd33084466f402d5d97f05f8e3b4af": "CHZ",
  "0x0f5d2fb29fb7d3cfee444a200298f468908cc942": "MANA",
  "0x3845badade8e6dff049820680d1f14bd3903a5d0": "SAND",
  "0x6810e776880c02933d47db1b9fc05908e5386b96": "GNO",
  "0xd33526068d116ce69f19a9ee46f0bd304f21a51f": "RPL",
  "0xae78736cd615f374d3085123a210448e74fc6393": "rETH",
  "0xbe9895146f7af43049ca1c1ae358b0541ea49704": "cbETH",
  "0x853d955acef822db058eb8505911ed77f175b99e": "FRAX",
  "0x4c9edd5852cd905f086c759e8383e09bff1e68b3": "USDe",
  "0x57e114b691db790c35207b2e685d4a43181e6061": "ENA",
};

const TOKEN_DECIMALS = {
  "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599": 8,
  "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48": 6,
  "0xdac17f958d2ee523a2206206994597c13d831ec7": 6,
};

const DEFAULT_NODE_COLOR = { background: "#58a6ff", border: "#1f6feb" };
const SOURCE_NODE_COLOR = { background: "#2ea043", border: "#3fb950" };
const TARGET_NODE_COLOR = { background: "#da3633", border: "#f85149" };
const DIMMED_NODE_COLOR = { background: "#21262d", border: "#30363d" };

function parseReserveThreshold(value) {
  if (typeof value !== "string" || value.trim() === "") {
    return null;
  }
  try {
    const threshold = BigInt(value.trim());
    return threshold > 0n ? threshold : null;
  } catch {
    return null;
  }
}

function matchesSearch(token, query) {
  const symbol = tokenSymbol(token);
  return (
    token.toLowerCase().includes(query) ||
    (symbol ? symbol.toLowerCase().includes(query) : false)
  );
}

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
 * direction. Edges are hidden when their platform is filtered out or their
 * reserve is below the minimum. A non-empty `searchQuery` dims non-matching
 * tokens and edges.
 *
 * When `routeEdgeIds` is a non-empty set (the best route from `/quote`), those
 * hops are emphasised and every other edge is dimmed. Source/target tokens are
 * recoloured to show the selection.
 */
export function syncGraphData(nodes, edges, graph, options = {}) {
  const {
    enabledPlatforms,
    tokenIn,
    tokenOut,
    routeEdgeIds,
    showEdgeLabels,
    searchQuery,
    minLiquidity,
  } = options;
  if (!graph) {
    return;
  }

  const query = typeof searchQuery === "string" ? searchQuery.trim().toLowerCase() : "";
  const searchActive = query.length > 0;
  const minReserve = parseReserveThreshold(minLiquidity);

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
    const selected =
      (sourceId !== null && tokenId === sourceId) || (targetId !== null && tokenId === targetId);
    const matches = !searchActive || matchesSearch(token, query);
    let color = DEFAULT_NODE_COLOR;
    if (sourceId !== null && tokenId === sourceId) {
      color = SOURCE_NODE_COLOR;
    } else if (targetId !== null && tokenId === targetId) {
      color = TARGET_NODE_COLOR;
    } else if (!matches) {
      color = DIMMED_NODE_COLOR;
    }
    const symbol = tokenSymbol(token);
    nodes.update({
      id: token,
      label: symbol ?? shortAddress(token),
      title: `${symbol ? `${symbol} (${token})` : token}\n${degrees.get(token) ?? 0} directed edges`,
      color,
      font: { color: matches || selected ? "#e6edf3" : "#484f58" },
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
    const touchesSearch =
      !searchActive || matchesSearch(edge.from, query) || matchesSearch(edge.to, query);

    let hidden = enabledPlatforms ? !enabledPlatforms.has(edge.platform) : false;
    if (!hidden && minReserve !== null) {
      if (edge.reserve === undefined) {
        hidden = true;
      } else {
        try {
          hidden = BigInt(edge.reserve) < minReserve;
        } catch {
          hidden = true;
        }
      }
    }

    let width = liquidityWidth(edge.reserve);
    let alpha = priceRatio === null ? 1 : 0.4 + 0.6 * priceRatio;
    let fontColor = platformColor;
    let shadow = { enabled: false };
    if (!touchesSearch) {
      width = Math.max(0.3, width * 0.6);
      alpha = Math.min(alpha, 0.05);
      fontColor = "rgba(230, 237, 243, 0.12)";
    }
    if (highlighted) {
      width = Math.max(2.6, width + 0.4);
      alpha = 1;
      shadow = { enabled: true, color: platformColor, size: 5 };
    } else if (dimmed) {
      width = Math.max(0.4, width * 0.5);
      alpha = Math.min(alpha, 0.12);
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
      hidden,
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
