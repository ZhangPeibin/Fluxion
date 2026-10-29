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

export function shortAddress(address) {
  if (typeof address !== "string" || address.length < 12) {
    return String(address);
  }
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
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
 * Map the service's `/graph` payload onto vis-network datasets.
 *
 * Parallel pool hops (same token pair, multiple pools/platforms) get their own
 * curve by bending each additional parallel edge further out; opposite
 * directions curve the other way so A→B and B→A stay distinguishable.
 */
export function buildGraphData(graph) {
  const nodes = new DataSet();
  const edges = new DataSet();
  if (!graph) {
    return { nodes, edges };
  }

  for (const token of graph.tokens ?? []) {
    nodes.add({
      id: token,
      label: shortAddress(token),
      title: token,
    });
  }

  const parallelCount = new Map();
  (graph.edges ?? []).forEach((edge, index) => {
    const key = `${edge.from}->${edge.to}`;
    const parallelIndex = parallelCount.get(key) ?? 0;
    parallelCount.set(key, parallelIndex + 1);

    const color = PLATFORM_COLORS[edge.platform] ?? FALLBACK_COLOR;
    const reverse = edge.from > edge.to;
    edges.add({
      id: `e${index}`,
      from: edge.from,
      to: edge.to,
      label: `${edge.platform} ${shortAddress(edge.pool)}`,
      title: edgeTooltip(edge),
      color: { color, highlight: color, hover: color },
      font: { color },
      smooth: {
        enabled: true,
        type: reverse ? "curvedCCW" : "curvedCW",
        roundness: 0.12 + 0.1 * parallelIndex,
      },
    });
  });

  return { nodes, edges };
}
