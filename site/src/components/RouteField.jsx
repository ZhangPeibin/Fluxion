import { useEffect, useRef } from "react";

const TOKENS = [
  { symbol: "ETH", x: 0.5, y: 0.13, color: "#7c5cff" },
  { symbol: "USDC", x: 0.87, y: 0.28, color: "#3fb950" },
  { symbol: "WBTC", x: 0.9, y: 0.7, color: "#f7931a" },
  { symbol: "LDO", x: 0.66, y: 0.88, color: "#58a6ff" },
  { symbol: "DAI", x: 0.33, y: 0.86, color: "#e3b341" },
  { symbol: "wstETH", x: 0.13, y: 0.6, color: "#a371f7" },
  { symbol: "USDT", x: 0.16, y: 0.27, color: "#2ea043" },
];

const EDGES = [
  [0, 1, "uniswap-v3", 1],
  [0, 2, "uniswap-v2", -1],
  [0, 3, "curve", 1],
  [0, 5, "uniswap-v3", -1],
  [1, 3, "uniswap-v3", 1],
  [1, 6, "curve", -1],
  [2, 5, "uniswap-v2", 1],
  [3, 4, "uniswap-v3", -1],
  [4, 5, "curve", 1],
  [4, 6, "uniswap-v2", -1],
  [2, 4, "curve", 1],
  [1, 2, "uniswap-v3", 1],
  [3, 6, "uniswap-v3", -1],
];

const PLATFORM_COLOR = {
  "uniswap-v3": "#4d9dff",
  "uniswap-v2": "#22d3ee",
  curve: "#34d399",
};

const PACKET_COUNT = 22;

function hexToRgba(hex, alpha) {
  const value = hex.replace("#", "");
  const r = parseInt(value.slice(0, 2), 16);
  const g = parseInt(value.slice(2, 4), 16);
  const b = parseInt(value.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function quadPoint(a, c, b, t) {
  const mt = 1 - t;
  return {
    x: mt * mt * a.x + 2 * mt * t * c.x + t * t * b.x,
    y: mt * mt * a.y + 2 * mt * t * c.y + t * t * b.y,
  };
}

export default function RouteField({ hint }) {
  const wrapRef = useRef(null);
  const canvasRef = useRef(null);
  const hintRef = useRef(null);

  useEffect(() => {
    const wrap = wrapRef.current;
    const canvas = canvasRef.current;
    const hintNode = hintRef.current;
    const ctx = canvas?.getContext("2d");
    if (!wrap || !canvas || !ctx) return undefined;

    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const pointer = { x: -999, y: -999, active: false };
    const packets = Array.from({ length: PACKET_COUNT }, () => ({
      edge: Math.floor(Math.random() * EDGES.length),
      t: Math.random(),
      speed: 0.00009 + Math.random() * 0.00016,
    }));

    let width = 0;
    let height = 0;
    let dpr = 1;
    let raf = 0;
    let running = true;
    let last = 0;
    let visible = true;

    function resize() {
      const rect = wrap.getBoundingClientRect();
      width = Math.max(rect.width, 1);
      height = Math.max(rect.height, 1);
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      draw();
    }

    function nodePoint(index) {
      return {
        x: TOKENS[index].x * width,
        y: TOKENS[index].y * height,
      };
    }

    function geometry(edge) {
      const a = nodePoint(edge[0]);
      const b = nodePoint(edge[1]);
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const length = Math.hypot(dx, dy) || 1;
      const bend = 0.16 * length * (edge[3] ?? 1);
      return {
        a,
        b,
        c: {
          x: (a.x + b.x) / 2 - (dy / length) * bend,
          y: (a.y + b.y) / 2 + (dx / length) * bend,
        },
      };
    }

    function nearestNode() {
      if (!pointer.active) return -1;
      let best = -1;
      let bestDistance = 34;
      for (let index = 0; index < TOKENS.length; index += 1) {
        const point = nodePoint(index);
        const distance = Math.hypot(point.x - pointer.x, point.y - pointer.y);
        if (distance < bestDistance) {
          bestDistance = distance;
          best = index;
        }
      }
      return best;
    }

    function draw() {
      ctx.clearRect(0, 0, width, height);
      const hovered = nearestNode();
      const geometries = EDGES.map(geometry);

      EDGES.forEach((edge, index) => {
        const geo = geometries[index];
        const color = PLATFORM_COLOR[edge[2]] ?? "#58a6ff";
        const touchesHover = hovered === edge[0] || hovered === edge[1];
        ctx.beginPath();
        ctx.moveTo(geo.a.x, geo.a.y);
        ctx.quadraticCurveTo(geo.c.x, geo.c.y, geo.b.x, geo.b.y);
        ctx.strokeStyle = hexToRgba(color, touchesHover ? 0.55 : 0.16);
        ctx.lineWidth = touchesHover ? 1.6 : 1;
        ctx.stroke();
      });

      for (const packet of packets) {
        const geo = geometries[packet.edge];
        const edge = EDGES[packet.edge];
        const color = PLATFORM_COLOR[edge[2]] ?? "#58a6ff";
        const head = quadPoint(geo.a, geo.c, geo.b, packet.t);
        const tail = quadPoint(geo.a, geo.c, geo.b, Math.max(0, packet.t - 0.06));
        const gradient = ctx.createLinearGradient(tail.x, tail.y, head.x, head.y);
        gradient.addColorStop(0, hexToRgba(color, 0));
        gradient.addColorStop(1, hexToRgba(color, 0.85));
        ctx.beginPath();
        ctx.moveTo(tail.x, tail.y);
        ctx.lineTo(head.x, head.y);
        ctx.strokeStyle = gradient;
        ctx.lineWidth = 1.8;
        ctx.stroke();

        ctx.beginPath();
        ctx.arc(head.x, head.y, 2.1, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.shadowColor = color;
        ctx.shadowBlur = 10;
        ctx.fill();
        ctx.shadowBlur = 0;
      }

      for (let index = 0; index < TOKENS.length; index += 1) {
        const token = TOKENS[index];
        const point = nodePoint(index);
        const isHovered = hovered === index;
        const radius = isHovered ? 7.5 : 5.2;

        if (isHovered) {
          ctx.beginPath();
          ctx.arc(point.x, point.y, 17, 0, Math.PI * 2);
          ctx.strokeStyle = hexToRgba(token.color, 0.35);
          ctx.lineWidth = 1;
          ctx.stroke();
        }

        ctx.beginPath();
        ctx.arc(point.x, point.y, radius, 0, Math.PI * 2);
        ctx.fillStyle = token.color;
        ctx.shadowColor = token.color;
        ctx.shadowBlur = isHovered ? 18 : 8;
        ctx.fill();
        ctx.shadowBlur = 0;

        ctx.font = `${isHovered ? "600 12px" : "500 11px"} ui-monospace, SFMono-Regular, Menlo, monospace`;
        ctx.fillStyle = isHovered ? "#f2f5f9" : "#8b98a9";
        ctx.textAlign = "center";
        ctx.fillText(token.symbol, point.x, point.y + 22);
      }

      if (!reduceMotion && hintNode) {
        hintNode.classList.toggle("is-engaged", hovered >= 0);
      }
    }

    function frame(now) {
      if (!running) return;
      const delta = last ? Math.min(now - last, 64) : 16;
      last = now;
      if (!reduceMotion) {
        for (const packet of packets) {
          packet.t += packet.speed * delta;
          if (packet.t >= 1) {
            packet.t = 0;
            packet.edge = Math.floor(Math.random() * EDGES.length);
          }
        }
      }
      draw();
      if (!reduceMotion && visible) raf = requestAnimationFrame(frame);
    }

    function onPointerMove(event) {
      const rect = wrap.getBoundingClientRect();
      pointer.x = event.clientX - rect.left;
      pointer.y = event.clientY - rect.top;
      pointer.active = true;
    }

    function onPointerLeave() {
      pointer.active = false;
      pointer.x = -999;
      pointer.y = -999;
    }

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(wrap);

    const visibilityObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          visible = entry.isIntersecting;
          if (visible && !running) {
            running = true;
            last = 0;
            raf = requestAnimationFrame(frame);
          }
          if (!visible) {
            running = false;
            cancelAnimationFrame(raf);
          }
        }
      },
      { threshold: 0.05 },
    );
    visibilityObserver.observe(wrap);

    wrap.addEventListener("pointermove", onPointerMove, { passive: true });
    wrap.addEventListener("pointerleave", onPointerLeave);
    resize();
    if (!reduceMotion) {
      last = 0;
      raf = requestAnimationFrame(frame);
    }

    return () => {
      running = false;
      cancelAnimationFrame(raf);
      resizeObserver.disconnect();
      visibilityObserver.disconnect();
      wrap.removeEventListener("pointermove", onPointerMove);
      wrap.removeEventListener("pointerleave", onPointerLeave);
    };
  }, []);

  return (
    <div className="route-field" ref={wrapRef}>
      <canvas ref={canvasRef} aria-hidden="true" />
      <div className="route-field-badges" aria-hidden="true">
        <span className="chip chip-blue">swap.quote</span>
        <span className="chip chip-green">wallet.balance</span>
        <span className="chip chip-violet">transaction.inspect</span>
      </div>
      <p className="route-field-hint" ref={hintRef}>
        <span className="pulse-dot" aria-hidden="true" />
        {hint}
      </p>
    </div>
  );
}
