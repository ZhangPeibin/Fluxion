const FOURMEME_TOKEN_URL = "https://four.meme/token/";
const BSCSCAN_URL = "https://bscscan.com/address/";
const ADDRESS_PATTERN = /^0x[0-9a-fA-F]{40}$/;

const state = {
  tokens: [],
  meta: null,
  filter: "all",
  search: "",
  loading: false,
  detail: null,
  detailAddress: null,
};

const elements = {
  lookback: document.getElementById("lookback"),
  wallet: document.getElementById("wallet"),
  refresh: document.getElementById("refresh"),
  meta: document.getElementById("meta"),
  tabs: document.getElementById("tabs"),
  search: document.getElementById("search"),
  status: document.getElementById("status"),
  warnings: document.getElementById("warnings"),
  list: document.getElementById("list"),
  listView: document.getElementById("list-view"),
  detailView: document.getElementById("detail-view"),
  detail: document.getElementById("detail"),
};

function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined && text !== null) node.textContent = String(text);
  return node;
}

function shortAddress(address) {
  if (!address || address.length < 12) return address || "";
  return `${address.slice(0, 6)}…${address.slice(-4)}`;
}

function phaseText(phase) {
  if (phase === "bonding") return "Bonding";
  if (phase === "graduated") return "Graduated";
  return "Unknown";
}

function phaseClass(phase) {
  return phase === "bonding" || phase === "graduated" ? phase : "unknown";
}

function formatChange(market) {
  if (!market) return "—";
  if (market.change_24h) return market.change_24h;
  if (market.change_24h_percent !== undefined) return `${market.change_24h_percent}%`;
  return "—";
}

function badge(text, className) {
  return element("span", `badge ${className || ""}`.trim(), text);
}

function eligibilityBadge(token) {
  if (token.wallet_eligible === true) return badge("钱包可参与", "good");
  if (token.wallet_eligible === false) return badge("钱包无 Agent ID", "bad");
  return badge("资格未确认", "unknown");
}

function setStatus(message, isError) {
  elements.status.hidden = !message;
  elements.status.textContent = message || "";
  elements.status.classList.toggle("error", Boolean(isError));
}

function setWarnings(warnings) {
  const values = (warnings || []).filter(Boolean);
  elements.warnings.hidden = values.length === 0;
  elements.warnings.replaceChildren();
  if (values.length === 0) return;
  const title = element("strong", null, "Warnings");
  elements.warnings.append(title);
  for (const warning of values) {
    elements.warnings.append(element("div", null, warning));
  }
}

function setMeta(data) {
  if (!data) {
    elements.meta.textContent = "准备加载…";
    return;
  }
  const lines = [
    `scanned: ${data.scanned} events`,
    `blocks: ${data.from_block} - ${data.to_block}`,
    `lookback: ${data.lookback_blocks}`,
    `coverage: ${data.coverage}`,
  ];
  elements.meta.textContent = lines.join("\n");
}

async function api(path) {
  const response = await fetch(path, { cache: "no-store" });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(body?.error?.message || `HTTP ${response.status}`);
  }
  return body;
}

function validateWallet() {
  const value = elements.wallet.value.trim();
  if (value && !ADDRESS_PATTERN.test(value)) {
    setStatus("钱包地址格式无效，应为 0x 开头的 40 位十六进制地址。", true);
    return false;
  }
  return true;
}

async function loadConfig() {
  try {
    const config = await api("/api/config");
    elements.lookback.value = String(config.lookback_blocks || 100000);
    elements.wallet.value = config.wallet || "";
  } catch (error) {
    setStatus(`加载配置失败：${error.message}`, true);
  }
}

async function loadTokens(force = false) {
  if (!validateWallet()) return;
  const lookback = Number(elements.lookback.value);
  if (!Number.isInteger(lookback) || lookback < 1 || lookback > 5000000) {
    setStatus("回看区块必须在 1 到 5000000 之间。", true);
    return;
  }
  state.loading = true;
  setStatus("正在扫描 BSC 日志并读取 Four.meme 详情…", false);
  setWarnings([]);
  elements.list.replaceChildren(
    ...Array.from({ length: 6 }, () => element("div", "skeleton")),
  );
  try {
    const query = new URLSearchParams({
      lookback_blocks: String(lookback),
    });
    if (force) query.set("refresh", "1");
    if (elements.wallet.value.trim()) {
      query.set("wallet", elements.wallet.value.trim());
    }
    const data = await api(`/api/tokens?${query.toString()}`);
    state.tokens = data.tokens || [];
    state.meta = data;
    setMeta(data);
    setWarnings(data.warnings);
    setStatus(data.tokens.length ? "" : "当前回看范围内没有匹配的 Agent ID Exclusive token。", false);
    renderList();
  } catch (error) {
    state.tokens = [];
    state.meta = null;
    setMeta(null);
    setStatus(`加载失败：${error.message}`, true);
    renderList();
  } finally {
    state.loading = false;
  }
}

function renderList() {
  const list = elements.list;
  list.replaceChildren();
  const search = state.search.toLowerCase();
  const tokens = state.tokens.filter((token) => {
    if (state.filter !== "all" && token.phase !== state.filter) return false;
    if (!search) return true;
    return [token.name, token.symbol, token.address]
      .filter(Boolean)
      .some((value) => value.toLowerCase().includes(search));
  });

  if (!tokens.length) {
    list.append(
      element(
        "div",
        "empty",
        state.loading ? "正在加载…" : "没有符合条件的 token。",
      ),
    );
    return;
  }

  for (const token of tokens) {
    const card = element("a", "card");
    card.href = `#/token/${token.address}`;

    const head = element("div", "card-head");
    const thumb = element("div", "thumb", (token.symbol || token.name || "?").slice(0, 1));
    if (token.image) {
      const image = element("img");
      image.src = token.image;
      image.alt = "";
      image.loading = "lazy";
      image.referrerPolicy = "no-referrer";
      image.addEventListener("error", () => image.remove());
      thumb.append(image);
    }
    const title = element("div", "card-title");
    title.append(
      element("strong", null, token.name || "未命名 token"),
      element("span", null, token.symbol || shortAddress(token.address)),
    );
    head.append(thumb, title);

    const foot = element("div", "card-foot");
    foot.append(
      badge(phaseText(token.phase), phaseClass(token.phase)),
      badge("Agent ID", "graduated"),
    );
    if (token.wallet_eligible !== undefined) foot.append(eligibilityBadge(token));

    const metric = element("div", "card-metric");
    metric.append(element("strong", null, token.raised || "—"), document.createTextNode(" BNB raised"));

    card.append(head, foot, metric);
    list.append(card);
  }
}

async function route() {
  const hash = window.location.hash || "#/";
  const match = hash.match(/^#\/token\/(0x[0-9a-fA-F]{40})$/);
  if (match) {
    elements.listView.hidden = true;
    elements.detailView.hidden = false;
    await loadDetail(match[1]);
  } else {
    elements.detailView.hidden = true;
    elements.listView.hidden = false;
    elements.detail.replaceChildren();
    if (!state.tokens.length && !state.loading) await loadTokens();
  }
}

async function loadDetail(address, force = false) {
  const previous = state.detailAddress === address ? state.detail : null;
  state.detailAddress = address;
  state.detail = null;
  elements.detail.replaceChildren(renderDetailLoading(address, previous));
  if (!validateWallet()) return;
  const query = new URLSearchParams();
  if (force) query.set("refresh", "1");
  if (elements.wallet.value.trim()) query.set("wallet", elements.wallet.value.trim());
  try {
    const suffix = query.toString() ? `?${query.toString()}` : "";
    const detail = await api(`/api/token/${address}${suffix}`);
    state.detail = detail;
    elements.detail.replaceChildren(renderDetail(detail));
  } catch (error) {
    const fallback = state.tokens.find((token) => token.address.toLowerCase() === address.toLowerCase());
    elements.detail.replaceChildren(renderDetailError(address, error.message, fallback));
  }
}

function renderDetailLoading(address, previous) {
  const wrap = element("div", "detail");
  const nav = element("div", "detail-nav");
  const back = element("a", "back", "← 返回列表");
  back.href = "#/";
  nav.append(back, element("span", "reason", shortAddress(address)));
  wrap.append(nav);
  if (previous) {
    wrap.append(renderDetail(previous));
  } else {
    wrap.append(element("div", "skeleton"));
  }
  wrap.append(element("div", "status", "正在读取 token 详情…"));
  return wrap;
}

function renderDetailError(address, message, fallback) {
  const wrap = element("div", "detail");
  const nav = element("div", "detail-nav");
  const back = element("a", "back", "← 返回列表");
  back.href = "#/";
  nav.append(back, element("span", "reason", shortAddress(address)));
  const card = element("div", "detail-card");
  card.append(
    element("h2", null, fallback?.name || "Token 详情"),
    element("p", "lede", `详情暂不可用：${message}`),
  );
  if (fallback) {
    card.append(
      element("div", "reason", `日志元数据：${fallback.symbol || "—"} · ${fallback.phase}`),
    );
  }
  wrap.append(nav, card);
  return wrap;
}

function stat(label, value) {
  const node = element("div", "stat");
  node.append(element("span", null, label), element("strong", null, value || "—"));
  return node;
}

function formatTime(timestamp) {
  if (!timestamp) return "—";
  const date = new Date(Number(timestamp));
  if (Number.isNaN(date.getTime())) return String(timestamp);
  return date.toLocaleString();
}

function renderDetail(detail) {
  const wrap = element("div", "detail");
  const nav = element("div", "detail-nav");
  const back = element("a", "back", "← 返回列表");
  back.href = "#/";
  nav.append(back);
  const refresh = element("button", "copy", "刷新详情");
  refresh.type = "button";
  refresh.addEventListener("click", () => loadDetail(detail.address, true));
  nav.append(refresh);

  const card = element("div", "detail-card");
  const head = element("div", "detail-head");
  const thumb = element("div", "thumb detail-thumb", (detail.symbol || detail.name || "?").slice(0, 1));
  if (detail.image) {
    const image = element("img");
    image.src = detail.image;
    image.alt = "";
    image.referrerPolicy = "no-referrer";
    image.addEventListener("error", () => image.remove());
    thumb.append(image);
  }
  const title = element("div", "detail-title");
  title.append(element("h2", null, detail.name || "未命名 token"));
  title.append(element("div", "detail-symbol", detail.symbol || "—"));
  const badges = element("div", "detail-badges");
  badges.append(badge(phaseText(detail.phase), phaseClass(detail.phase)));
  if (detail.agent_id_exclusive === true) badges.append(badge("Agent ID Exclusive", "graduated"));
  if (detail.agent_id_exclusive === false) badges.append(badge("非 Agent ID Exclusive", "unknown"));
  if (detail.agent_id_exclusive === undefined) badges.append(badge("模板未确认", "unknown"));
  if (detail.wallet_eligible === true) badges.append(badge("钱包可参与", "good"));
  if (detail.wallet_eligible === false) badges.append(badge("钱包无 Agent ID", "bad"));
  title.append(badges);

  const addressRow = element("div", "address-row");
  addressRow.append(element("span", null, detail.address));
  const copy = element("button", "copy", "复制");
  copy.type = "button";
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(detail.address);
      copy.textContent = "已复制";
    } catch {
      copy.textContent = "复制失败";
    }
  });
  addressRow.append(copy);

  const links = element("div", "links");
  links.append(externalLink("Four.meme", `${FOURMEME_TOKEN_URL}${detail.address}`));
  links.append(externalLink("BscScan", `${BSCSCAN_URL}${detail.address}`));

  head.append(thumb, title);
  card.append(head, addressRow, links);

  const stats = element("div", "stats");
  const raised = detail.market?.raised ?? detail.raised;
  stats.append(
    stat("Raised", raised ? `${raised} BNB` : "—"),
    stat("Market cap", detail.market?.market_cap ?? detail.market_cap),
    stat("Price", detail.market?.price ?? detail.price),
    stat("Max price", detail.market?.max_price),
    stat("Liquidity", detail.market?.liquidity),
    stat("24h volume", detail.market?.volume_24h),
    stat("24h change", formatChange(detail.market)),
    stat("Total supply", detail.total_supply),
    stat("Sale amount", detail.sale_amount),
    stat("Phase", phaseText(detail.phase)),
    stat("Creator", detail.creator ? shortAddress(detail.creator) : "—"),
    stat("Created", formatTime(detail.created_at)),
    stat("Launch", formatTime(detail.launch_at)),
    stat("Version", detail.version),
  );
  card.append(stats);

  if (detail.tradability || detail.wallet_cap) {
    const limits = element("div", "section");
    limits.append(element("h3", null, "交易限制"));
    const limitStats = element("div", "stats");
    if (detail.tradability?.progress_percent !== undefined) {
      limitStats.append(stat("曲线进度", `${detail.tradability.progress_percent}%`));
    }
    if (detail.tradability?.transfer_locked !== undefined) {
      limitStats.append(stat("内盘转账", detail.tradability.transfer_locked ? "锁定" : "开放"));
    }
    if (detail.tradability?.max_supply) {
      limitStats.append(stat("创建时 maxSupply", detail.tradability.max_supply.decimal));
    }
    if (detail.wallet_cap) {
      limitStats.append(
        stat("钱包余额", detail.wallet_cap.balance.decimal),
        stat("1% 上限", detail.wallet_cap.cap.decimal),
        stat("剩余额度", detail.wallet_cap.remaining.decimal),
        stat("已达上限", detail.wallet_cap.reached ? "是" : "否"),
      );
    }
    limits.append(limitStats);
    card.append(limits);
  }

  const rule = element("div", "section");
  rule.append(element("h3", null, "Agent ID Exclusive 规则"));
  rule.append(
    element(
      "p",
      null,
      "内盘阶段仅 Binance Agentic Wallet + Agent ID 可交易；单钱包最多持有创建时 maxSupply 的 1%；内盘期间不可转账；毕业后开放交易。",
    ),
  );
  card.append(rule);

  if (detail.description) {
    const description = element("div", "section");
    description.append(element("h3", null, "项目描述"));
    description.append(element("p", null, detail.description));
    card.append(description);
  }

  if (detail.wallet_eligible !== undefined || detail.eligibility_reason) {
    const eligibility = element("div", "section");
    eligibility.append(element("h3", null, "钱包资格"));
    const row = element("div", "eligibility");
    if (detail.wallet_eligible === true) row.append(badge("可参与", "good"));
    if (detail.wallet_eligible === false) row.append(badge("不可参与", "bad"));
    if (detail.wallet_eligible === undefined) row.append(badge("未确认", "unknown"));
    row.append(element("span", "reason", detail.eligibility_reason || "—"));
    eligibility.append(row);
    card.append(eligibility);
  }

  if (detail.warnings?.length) {
    const warnings = element("div", "section");
    warnings.append(element("h3", null, "Warnings"));
    const list = element("ul", "warning-list");
    for (const warning of detail.warnings) list.append(element("li", null, warning));
    warnings.append(list);
    card.append(warnings);
  }

  const sources = element("div", "section");
  sources.append(element("h3", null, "Coverage"));
  sources.append(element("p", "reason", `${detail.coverage} · ${(detail.sources || []).map((source) => source.name).join(", ")}`));
  card.append(sources);

  wrap.append(nav, card);
  return wrap;
}

function externalLink(label, href) {
  const link = element("a", "link", label);
  link.href = href;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  return link;
}

function bindEvents() {
  elements.refresh.addEventListener("click", () => {
    if (window.location.hash.startsWith("#/token/")) {
      const address = window.location.hash.split("/").pop();
      loadDetail(address, true);
    } else {
      loadTokens(true);
    }
  });

  elements.tabs.addEventListener("click", (event) => {
    const button = event.target.closest("[data-filter]");
    if (!button) return;
    state.filter = button.dataset.filter;
    for (const tab of elements.tabs.querySelectorAll(".tab")) {
      tab.classList.toggle("active", tab === button);
    }
    renderList();
  });

  elements.search.addEventListener("input", () => {
    state.search = elements.search.value.trim();
    renderList();
  });

  elements.wallet.addEventListener("change", () => {
    state.tokens = [];
    if (window.location.hash.startsWith("#/token/")) {
      const address = window.location.hash.split("/").pop();
      loadDetail(address, true);
    } else {
      loadTokens(true);
    }
  });

  elements.lookback.addEventListener("change", () => {
    state.tokens = [];
    loadTokens(true);
  });

  window.addEventListener("hashchange", route);
}

async function main() {
  bindEvents();
  await loadConfig();
  await route();
}

main();
