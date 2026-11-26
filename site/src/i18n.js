export const GITHUB_URL = "https://github.com/ZhangPeibin/eollo";

const REPO = `${GITHUB_URL}/blob/master`;

export const MCP_CONFIGS = {
  claude: {
    path: ".mcp.json",
    code: `{
  "mcpServers": {
    "eollo": {
      "command": "eollo",
      "args": ["mcp"]
    }
  }
}`,
  },
  codex: {
    path: "~/.codex/config.toml",
    code: `[mcp_servers.eollo]
command = "eollo"
args = ["mcp"]`,
  },
  opencode: {
    path: "opencode.json",
    code: `{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "eollo": {
      "type": "local",
      "command": ["eollo", "mcp"],
      "enabled": true
    }
  }
}`,
  },
};

export const INSTALL_COMMAND = "cargo install --path crates/client";
export const MCP_COMMAND = "eollo mcp";
export const SKILLS_COMMAND = "npx skills add ZhangPeibin/eollo -g -a opencode";

export const copy = {
  zh: {
    captions: {
      langSwitch: "EN",
      langHref: "./",
      openMenu: "打开菜单",
      copy: "复制",
      copied: "已复制",
      all: "全部",
      skillLink: "查看 SKILL.md",
      docsLink: "能力文档",
    },
    nav: {
      brand: "Eollo",
      tagline: "Web3 capability layer",
      links: [
        { href: "#problems", label: "产品" },
        { href: "#capabilities", label: "能力" },
        { href: "#install", label: "安装" },
        { href: "#skills", label: "Skills" },
        { href: "#safety", label: "安全" },
      ],
      cta: "开始接入",
    },
    hero: {
      eyebrow: "MCP stdio · Pre-1.0",
      title: {
        before: "给 Agent 的 ",
        highlight: "Web3 能力层",
        after: "",
        breakAfterHighlight: false,
      },
      tagline: {
        line1: "一边是 Agent，一边是链。",
        line2Before: "中间，是 ",
        highlight: "Eollo",
        line2After: "。",
      },
      installLabel: "一条命令开始",
      installCta: "安装 CLI",
      primary: "查看安装步骤",
      secondary: "GitHub 仓库",
      stats: [
        { key: "capabilities", label: "个能力" },
        { key: "domains", label: "个领域" },
        { value: "2", label: "条链 · Ethereum / BSC" },
        { value: "7", label: "个 Skills" },
      ],
      canvasHint: "路由多重图 · 悬停查看节点",
      scrollCue: "向下滚动",
    },
    map: {
      ariaLabel: "全部能力的动态图，按领域分组",
      workflows: "工作流",
      requires: "依赖",
      touring: "演示",
      viewInCatalog: "在能力目录查看",
      close: "关闭",
      session: "wallet.session",
      staticHint: "共 24 个能力，按领域分布",
    },
    problems: {
      eyebrow: "为什么需要它",
      title: "3 个问题，一层解决",
      items: [
        {
          title: "每个协议都是一次新集成",
          body: "DeFi 的碎片化落到 Agent 身上，就是 N 个协议 × M 条链 × K 个钱包。每接一个，都要重写一套胶水代码。",
          points: ["一个 capability envelope", "固定的链与执行规则", "稳定的错误码"],
        },
        {
          title: "每个 Agent 动作都是一次信任判断",
          body: "用户看不懂 calldata，也无法预判资产变化。签名前必须能回答：它在做什么、会动哪些资产、哪些没有检查到。",
          points: ["decode / simulate / inspect", "coverage 与 unknowns", "advisory，不冒充保证"],
        },
        {
          title: "自主需要一次交接",
          body: "让 Agent 直接持有私钥不可接受。Eollo 让 Agent 准备交易，把最后一步交给手机钱包确认；私钥始终不进入运行时。",
          points: ["私钥留在手机", "WalletConnect 会话", "每次发送都显式确认"],
        },
      ],
    },
    flow: {
      eyebrow: "工作流",
      title: "一次 swap 如何穿过 Eollo",
      lead: "每个步骤都是一个独立能力，按目录顺序组合；不是隐藏的执行路径。",
      note: "执行位置由 Eollo 固定：swap.quote 走 remote 服务，钱包与交易检查在本地运行。Agent 和用户不选择 remote/local。",
      cta: "查看 swap 工作流",
      routeTitle: "路由预览",
      routeNote: "示意图：真实路由多重图由 remote 服务导出。",
    },
    steps: [
      { id: "swap.quote", title: "找到最优路径", body: "Ethereum 主网 exact-in 报价，多跳与拆单，全部固定在同一区块。" },
      { id: "swap.build", title: "构建 calldata", body: "从已保存的报价构建 V2 swap calldata，本地 ABI 编码。" },
      { id: "wallet.allowance", title: "检查授权", body: "读取 ERC20/BEP-20 额度，零授权也会返回。" },
      { id: "transaction.simulate", title: "预演交易", body: "固定区块 eth_call 与 eth_estimateGas，返回 success/revert。" },
      { id: "transaction.inspect", title: "签名前检查", body: "解码意图、资产变化与风险 findings，并标注覆盖范围。" },
      { id: "transaction.send", title: "手机确认", body: "模拟与检查通过后，经 WalletConnect 交给手机钱包广播。" },
      { id: "transaction.verify", title: "核对回执", body: "读取 receipt，解码实际 effects，可对照签名前的报告。" },
    ],
    capabilities: {
      eyebrow: "能力目录",
      title: "一个目录，覆盖七类链上动作",
      lead: "下面每张卡都来自 capabilities/catalog.yaml；状态、链和执行位置都写在契约里。",
      filterLabel: "按领域筛选",
      status: { stable: "stable", experimental: "experimental", proposed: "proposed", deprecated: "deprecated" },
      execution: { remote: "remote 服务", local: "本地运行" },
      chainsLabel: "链",
    },
    domains: {
      swap: { label: "报价", desc: "精确输入报价、多跳与拆单" },
      wallet: { label: "钱包", desc: "余额、授权、资产与 WalletConnect 会话" },
      transaction: { label: "交易安全", desc: "签名前检查、模拟、发送与回执核对" },
      signature: { label: "签名授权", desc: "EIP-712、Permit2、7702 与 4337 检查" },
      fourmeme: { label: "Four.meme", desc: "Agent ID Exclusive token 发现与详情" },
      yield: { label: "收益发现", desc: "ETH 质押/借贷机会、历史 APY 与 calldata" },
      position: { label: "仓位分析", desc: "跨 curated 市场的仓位、估值与 PnL" },
    },
    capabilitySummaries: {
      "swap.quote": "以太坊 exact-in 报价。",
      "swap.build": "从报价构建 V2 swap calldata。",
      "wallet.balance": "在固定区块读取原生币与代币余额。",
      "wallet.assets": "发现 ERC20 持仓并在本地核对余额。",
      "wallet.allowance": "读取 owner 给 spender 的 ERC20 授权。",
      "wallet.collectibles": "读取 ERC721 与 ERC1155 持仓。",
      "wallet.connect": "发起 WalletConnect v2 配对。",
      "wallet.status": "读取本地 WalletConnect 会话。",
      "wallet.disconnect": "删除本地 WalletConnect 会话。",
      "wallet.agent-id": "解析 BSC .ai Agent ID。",
      "wallet.sign": "通过钱包会话签名消息或 EIP-712。",
      "wallet.send": "模拟并确认后发送原生币或 ERC20 转账。",
      "transaction.simulate": "在固定区块模拟交易。",
      "transaction.inspect": "签名前检查未签名 calldata。",
      "transaction.send": "模拟、检查后通过钱包会话发送。",
      "transaction.verify": "核对回执，并把实际 effects 与检查报告对照。",
      "signature.inspect": "检查 typed data、授权列表与 user operation。",
      "fourmeme.agent-tokens": "发现 Four.meme Agent ID Exclusive token，支持按区块翻页。",
      "fourmeme.token": "读取单个 token 的阶段、可交易性与钱包资格。",
      "yield.discover": "发现 curated ETH 质押与借贷机会。",
      "yield.build": "为 curated 机会构建存入 calldata 与授权步骤。",
      "yield.history": "读取 curated 机会的历史 APY 与 TVL。",
      "yield.withdraw": "为 curated 收益仓位构建退出 calldata。",
      "position.analyze": "分析钱包在 curated ETH 收益市场的仓位。",
    },
    install: {
      eyebrow: "接入",
      title: "两分钟接入",
      lead: "Eollo 通过 MCP stdio 暴露能力。客户端只负责启动进程，能力路由由 Eollo 内部完成。",
      steps: [
        { title: "安装 CLI", body: "需要 Rust 工具链；安装后 eollo 必须在 PATH 中。" },
        { title: "启动 MCP", body: "stdio 暴露已安装的能力，不会启动 remote 服务。" },
        { title: "注册到你的 Agent", body: "把 eollo mcp 写进客户端配置，然后重启 Agent。" },
      ],
      remote: {
        title: "Remote 服务",
        lead: "报价、收益和仓位分析通过 remote 完成；钱包和交易检查始终在本地。",
        tabs: { managed: "托管端点", selfHost: "自托管" },
        badge: "即将上线",
        managed: {
          scope: "覆盖 swap.quote、yield.discover、yield.history、position.analyze，全部在 Ethereum 主网。",
          authNote: "认证由客户端自动完成：首次调用自动 /register，无需手动填写 key。",
          pending: "托管端点正在准备中。上线前请使用自托管。",
          pendingCta: "查看自托管",
          docs: "Remote 接入文档",
        },
        selfHost: {
          installCommand: "cargo install --path crates/remote --bin eollo-remote",
          serveCommand: "EOLLO_REQUIRE_AUTH=1 eollo-remote serve --config configs/runtime.yaml",
          secretNote: "GRAPH_API_KEY 和 RPC_URL 只放服务端 .env，永远不要给客户端。",
          warmupNote: "首次启动会发现池子并同步 local snapshot，需要一些时间。",
          docs: "配置文档",
        },
      },
      mcpTitle: "选择你的客户端",
      mcpLead: "把下面的片段放进对应配置文件；字段名随客户端版本可能略有差异。",
    },
    skills: {
      eyebrow: "Agent 使用说明",
      title: "Skills 告诉 Agent 何时调用",
      lead: "Skills 不是执行路径，而是给 Agent 的指令：何时用哪个能力、如何解释结果、哪些边界不能越过。",
      configLabel: "用 skills CLI 安装（推荐）",
      listTitle: "七个 Skill",
      promptTitle: "直接对 Agent 说",
      promptHint: "点击任意一句复制",
      examplesLink: "查看完整示例",
      prompts: [
        "把 100 LDO 换成 ETH，给我最佳报价和路径。",
        "在 BSC 上把 1 个 BNB 换成 USDT，给我最优报价、路线和价格影响。",
        "把刚才的 BNB 换 USDT 报价构建成可签名的 calldata，滑点 0.5%。",
        "签名前检查这笔 calldata：它在做什么、会动哪些资产、有没有 delegatecall 风险？",
        "检查这份 Permit2 签名请求：spender、额度、过期时间和链 ID 是否正确？",
        "我有 10 ETH，按 conservative 找最安全的收益机会，并说明放弃了什么。",
        "我在 BSC 上有 10 个 BNB，找风险中等、收益不错的借贷机会，说明退出条件。",
        "看一下这个地址在 BSC 上的 Venus 仓位，值多少 BNB，APY 是多少。",
        "验证这笔 BSC 交易是否成功，实际转了什么。",
        "连接我的 BSC 钱包，先显示二维码，再轮询连接状态。",
      ],
    },
    skillsList: [
      { name: "swap", desc: "何时报价、何时构建 calldata，以及完整 swap 工作流。" },
      { name: "wallet", desc: "余额、资产、授权、WalletConnect 会话、转账与签名。" },
      { name: "transaction", desc: "calldata 检查、模拟、发送与回执验证。" },
      { name: "signature", desc: "EIP-712 / Permit2 / 7702 / 4337 的签名前检查。" },
      { name: "fourmeme", desc: "Agent ID Exclusive token 的发现、详情与资格。" },
      { name: "yield", desc: "ETH/BNB 收益发现、构建、历史与退出。" },
      { name: "position", desc: "读取地址在 curated 市场的仓位与 PnL。" },
    ],
    safety: {
      eyebrow: "安全模型",
      title: "签名前检查，不是安全保证",
      lead: "transaction.inspect 与 signature.inspect 只做 advisory 检查：不持有私钥、不签名、不广播。",
      items: [
        { title: "确定性", body: "同一请求、同一区块、同一代码版本产生相同结论。LLM 不参与 detector、risk 或 coverage 计算。" },
        { title: "覆盖与未知", body: "decode、metadata、simulation、trace、policy 五个维度独立报告；unknowns 是结果的一部分。" },
        { title: "风险语义", body: "没有 finding 时是 unknown，不是 low 或 safe；关键覆盖缺失时保持 unknown。" },
        { title: "强制执行有边界", body: "policy enforce 只覆盖 Eollo 自己的 send/sign 路径，不是通用 signer 边界。" },
      ],
      disclaimer: "Pre-1.0。能力范围与支持链仍在演进；检查结果用于辅助决策，不构成审计或安全保证。",
      cta: "阅读安全说明",
    },
    footer: {
      blurb: "给 Agent 的 Web3 能力层。固定契约、MCP 传输、签名前可审计。",
      columns: [
        {
          title: "产品",
          links: [
            { label: "能力目录", href: `${REPO}/capabilities/catalog.yaml` },
            { label: "Skills", href: `${REPO}/skills` },
          ],
        },
        {
          title: "文档",
          links: [
            { label: "MCP 接入", href: `${REPO}/docs/MCP.md` },
            { label: "HTTP API", href: `${REPO}/docs/API.md` },
            { label: "配置", href: `${REPO}/docs/CONFIGURATION.md` },
            { label: "安全说明", href: `${REPO}/docs/capabilities/transaction/ASSURANCE.md` },
          ],
        },
        {
          title: "项目",
          links: [{ label: "GitHub", href: GITHUB_URL }],
        },
      ],
      copyright: "Eollo · Pre-1.0",
    },
  },
  en: {
    captions: {
      langSwitch: "中文",
      langHref: "zh.html",
      openMenu: "Open menu",
      copy: "Copy",
      copied: "Copied",
      all: "All",
      skillLink: "View SKILL.md",
      docsLink: "Capability docs",
    },
    nav: {
      brand: "Eollo",
      tagline: "Web3 capability layer",
      links: [
        { href: "#problems", label: "Product" },
        { href: "#capabilities", label: "Capabilities" },
        { href: "#install", label: "Install" },
        { href: "#skills", label: "Skills" },
        { href: "#safety", label: "Safety" },
      ],
      cta: "Get started",
    },
    hero: {
      eyebrow: "MCP stdio · Pre-1.0",
      title: {
        before: "A Web3 ",
        highlight: "Layer",
        after: " for agents",
        breakAfterHighlight: true,
      },
      tagline: {
        line1: "Agent on one side. Chain on the other.",
        line2Before: "",
        highlight: "Eollo",
        line2After: " in between.",
      },
      installLabel: "Start with one command",
      installCta: "Install the CLI",
      primary: "See install steps",
      secondary: "GitHub repository",
      stats: [
        { key: "capabilities", label: "capabilities" },
        { key: "domains", label: "domains" },
        { value: "2", label: "chains · Ethereum / BSC" },
        { value: "7", label: "skills" },
      ],
      canvasHint: "Routing multigraph · hover a node",
      scrollCue: "Scroll to explore",
    },
    map: {
      ariaLabel: "Dynamic map of all capabilities, grouped by domain",
      workflows: "Workflows",
      requires: "Requires",
      touring: "Touring",
      viewInCatalog: "View in catalog",
      close: "Close",
      session: "wallet.session",
      staticHint: "24 capabilities grouped by domain",
    },
    problems: {
      eyebrow: "Why it exists",
      title: "3 problems, one layer",
      items: [
        {
          title: "Every protocol is a new integration",
          body: "DeFi fragmentation lands on the agent as N protocols × M chains × K wallets. Every connection starts with another bespoke integration.",
          points: ["One capability envelope", "Explicit chain and execution rules", "Stable error codes"],
        },
        {
          title: "Every agent action is a trust decision",
          body: "Users can't read calldata or predict asset changes. Before signing, they need to know what it does, what moves, and what was not checked.",
          points: ["decode / simulate / inspect", "coverage and unknowns", "advisory, not a guarantee"],
        },
        {
          title: "Autonomy needs a handoff",
          body: "Giving an agent the keys is not acceptable. Eollo prepares the transaction, then hands the final step to the phone wallet; keys never enter the runtime.",
          points: ["Keys stay on the phone", "WalletConnect session", "Explicit confirmation on every send"],
        },
      ],
    },
    flow: {
      eyebrow: "Workflow",
      title: "How one swap moves through Eollo",
      lead: "Each step is its own capability, composed in catalog order — never a hidden execution path.",
      note: "Execution location is fixed by Eollo: swap.quote uses the remote service; wallet and transaction checks run locally. Agents and users do not choose remote or local.",
      cta: "Read the swap workflow",
      routeTitle: "Route preview",
      routeNote: "Illustrative: the real routing multigraph comes from the remote service.",
    },
    steps: [
      { id: "swap.quote", title: "Find the best route", body: "Exact-in quoting on Ethereum mainnet with multi-hop and split routes, all pinned to one block." },
      { id: "swap.build", title: "Build calldata", body: "Build V2 swap calldata from a saved quote with local ABI encoding." },
      { id: "wallet.allowance", title: "Check allowance", body: "Read ERC20/BEP-20 allowances; zero approvals are returned too." },
      { id: "transaction.simulate", title: "Simulate", body: "eth_call and eth_estimateGas at a pinned block, returning success or revert." },
      { id: "transaction.inspect", title: "Inspect before signing", body: "Decode intent, asset changes and risk findings, and mark what was covered." },
      { id: "transaction.send", title: "Confirm on phone", body: "After simulation and inspection, hand off to the phone wallet over WalletConnect." },
      { id: "transaction.verify", title: "Verify the receipt", body: "Read the receipt, decode actual effects, and compare with the pre-signing report." },
    ],
    capabilities: {
      eyebrow: "Catalog",
      title: "One catalog, seven kinds of on-chain action",
      lead: "Every card below comes from capabilities/catalog.yaml; status, chains and execution are part of the contract.",
      filterLabel: "Filter by domain",
      status: { stable: "stable", experimental: "experimental", proposed: "proposed", deprecated: "deprecated" },
      execution: { remote: "remote service", local: "local runtime" },
      chainsLabel: "Chains",
    },
    domains: {
      swap: { label: "Swap", desc: "Exact-in quotes, multi-hop and split routes" },
      wallet: { label: "Wallet", desc: "Balances, allowances, assets and WalletConnect sessions" },
      transaction: { label: "Transaction safety", desc: "Inspect, simulate, send and verify" },
      signature: { label: "Signatures", desc: "EIP-712, Permit2, 7702 and 4337 inspection" },
      fourmeme: { label: "Four.meme", desc: "Agent ID Exclusive token discovery and detail" },
      yield: { label: "Yield", desc: "ETH staking and lending, history and calldata" },
      position: { label: "Positions", desc: "Curated market positions, value and PnL" },
    },
    install: {
      eyebrow: "Install",
      title: "Wired up in two minutes",
      lead: "Eollo exposes capabilities over MCP stdio. The client only starts a process; routing stays inside Eollo.",
      steps: [
        { title: "Install the CLI", body: "Rust toolchain required; eollo must be on PATH afterwards." },
        { title: "Start MCP", body: "stdio exposes the installed capabilities and never starts the remote server." },
        { title: "Register your agent", body: "Add eollo mcp to the client config, then restart the agent." },
      ],
      remote: {
        title: "Remote service",
        lead: "Quotes, yield and position analysis run on the remote service; wallet and transaction tools stay local.",
        tabs: { managed: "Managed endpoint", selfHost: "Self-host" },
        badge: "Soon",
        managed: {
          scope: "Covers swap.quote, yield.discover, yield.history and position.analyze, all on Ethereum.",
          authNote: "The client authenticates automatically: a 401 triggers /register, so no key is needed by hand.",
          pending: "The managed endpoint is being provisioned. Until it is live, self-host the remote service.",
          pendingCta: "Use self-host",
          docs: "Remote setup",
        },
        selfHost: {
          installCommand: "cargo install --path crates/remote --bin eollo-remote",
          serveCommand: "EOLLO_REQUIRE_AUTH=1 eollo-remote serve --config configs/runtime.yaml",
          secretNote: "GRAPH_API_KEY and RPC_URL live only in the server .env; never give them to the client.",
          warmupNote: "First start discovers pools and warms the local snapshot, so it takes a while.",
          docs: "Configuration",
        },
      },
      mcpTitle: "Pick your client",
      mcpLead: "Drop the snippet into the matching config file; key names can vary slightly by client version.",
    },
    skills: {
      eyebrow: "Agent instructions",
      title: "Skills tell the agent when to call",
      lead: "Skills are not an execution path. They are instructions for the agent: when to use a capability, how to read the result, and which boundaries not to cross.",
      configLabel: "Install with the skills CLI (recommended)",
      listTitle: "Seven skills",
      promptTitle: "Say this to your agent",
      promptHint: "Click a line to copy it",
      examplesLink: "See all examples",
      prompts: [
        "Swap 100 LDO to ETH and give me the best quote and route.",
        "Swap 1 BNB to USDT on BSC and show the best route and price impact.",
        "Build signable calldata from that BNB to USDT quote with 0.5% slippage.",
        "Inspect this calldata before I sign: what does it do, which assets move, and is there a delegatecall risk?",
        "Inspect this Permit2 request: are the spender, amount, expiration and chain ID correct?",
        "I have 10 ETH. Find the safest yield at a reasonable APY under conservative, and tell me what I gave up.",
        "I have 10 BNB on BSC. Find a lending market with a reasonable APY and explain the exit conditions.",
        "Show this address's Venus position on BSC: what is it worth in BNB, and what is the APY?",
        "Verify this BSC transaction: did it succeed, and what actually moved?",
        "Connect my BSC wallet, show the QR first, then poll the session status.",
      ],
    },
    skillsList: [
      { name: "swap", desc: "When to quote, when to build calldata, and the full swap workflow." },
      { name: "wallet", desc: "Balances, assets, allowances, WalletConnect sessions, transfers and signing." },
      { name: "transaction", desc: "Calldata inspection, simulation, sending and receipt verification." },
      { name: "signature", desc: "Pre-signing checks for EIP-712, Permit2, 7702 and 4337." },
      { name: "fourmeme", desc: "Discovery, detail and eligibility for Agent ID Exclusive tokens." },
      { name: "yield", desc: "ETH and BNB yield discovery, build, history and exit." },
      { name: "position", desc: "Read positions across curated markets with optional PnL." },
    ],
    safety: {
      eyebrow: "Security model",
      title: "Pre-signing inspection, not a safety guarantee",
      lead: "transaction.inspect and signature.inspect are advisory: they hold no keys, never sign and never broadcast.",
      items: [
        { title: "Deterministic", body: "The same request, same block and same code version produce the same findings. No LLM touches detectors, risk or coverage." },
        { title: "Coverage and unknowns", body: "decode, metadata, simulation, trace and policy are reported independently; unknowns are part of the result." },
        { title: "Risk semantics", body: "No findings means unknown, not low or safe. Missing critical coverage keeps it unknown." },
        { title: "Enforcement has borders", body: "Policy enforcement only covers Eollo's own send/sign paths, not a universal signer boundary." },
      ],
      disclaimer: "Pre-1.0. Capability scope and chain support are still moving; inspection helps a decision and is not an audit or a safety guarantee.",
      cta: "Read the assurance notes",
    },
    footer: {
      blurb: "A Web3 capability layer for agents. Fixed contracts, MCP transport, auditable pre-signing.",
      columns: [
        {
          title: "Product",
          links: [
            { label: "Catalog", href: `${REPO}/capabilities/catalog.yaml` },
            { label: "Skills", href: `${REPO}/skills` },
          ],
        },
        {
          title: "Docs",
          links: [
            { label: "MCP setup", href: `${REPO}/docs/MCP.md` },
            { label: "HTTP API", href: `${REPO}/docs/API.md` },
            { label: "Configuration", href: `${REPO}/docs/CONFIGURATION.md` },
            { label: "Assurance", href: `${REPO}/docs/capabilities/transaction/ASSURANCE.md` },
          ],
        },
        {
          title: "Project",
          links: [{ label: "GitHub", href: GITHUB_URL }],
        },
      ],
      copyright: "Eollo · Pre-1.0",
    },
  },
};
