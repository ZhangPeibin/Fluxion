# Configuration

产品配置集中在 `configs/`；benchmark 复现配置在 `bench/configs/`；本地测试 fixture 在 `tests/configs/`；根目录只保留 `config.example.yaml`。下表路径相对仓库根目录。

| 文件 | 用途 |
|---|---|
| `config.example.yaml` | 通用模板，复制为 `config.yaml` 后使用 |
| `configs/runtime.yaml` | CLI/MCP 默认暖 runtime（local snapshot + incremental） |
| `bench/configs/threeway.yaml` | 三方 benchmark 主配置（:8081） |
| `bench/configs/control.yaml` | 关闭转换/ShibaSwap 的对照配置（:8082） |
| `bench/configs/local.yaml` | local snapshot benchmark（:8089） |
| `tests/configs/v2.yaml` | 受限 V2/ShibaSwap 本地示例 |
| `tests/configs/v3.yaml` | V2/V3/ShibaSwap 本地示例 |
| `tests/configs/wsteth.yaml` | wstETH 转换与多跳示例 |
| `tests/configs/curve.yaml` | 显式经典 3pool |
| `tests/configs/curve-stable.yaml` | 三个已验证稳定池 |
| `tests/configs/tricrypto2.yaml` | 显式 Tricrypto2 |

| 键 | 说明 | 默认 |
|---|---|---|
| `host` / `port` | 监听地址 | `127.0.0.1` / `8080` |
| `rpc_url` | 第三方 RPC | `https://eth.drpc.org` |
| `uniswap_v2_subgraph_id` / `uniswap_v3_subgraph_id` | The Graph 去中心化网络部署 ID | 见示例 |
| `curve_address_provider` | Curve AddressProvider | `0x0000…4383` |
| `curve_registry_ids` | 枚举的 Curve 注册表 id（0=主 stableswap，5=crypto，7=MetaRegistry 全部） | `[0, 5]` |
| `enabled_platforms` | 启用的 AMM 平台；与转换列表独立 | `uniswap-v2` / `uniswap-v3` / `sushi-v3` / `pancake-v3` / `curve` / `shibaswap-v2`（示例配置） |
| `v3_deployments` | V3 兼容部署（venue / factory / quoter / `subgraph_id` 或 `deployment_id`）；内置 Uniswap V3 | 见示例 |
| `shibaswap_v2_factory` | ShibaSwap V2 工厂地址 | 见示例 |
| `protocol_conversions` | 已支持接口类型的部署实例；`[]` 关闭转换 | Lido stETH↔wstETH，双向开启 |
| `max_hops` / `top_k` / `max_pools` | 最大边数 / 每状态每能力组保留数 / 发现上限 | 3 / 8 / 10000 |
| `min_liquidity` | 流动性下限（USD 量级） | 10000 |
| `min_volume_24h` | 成交量下限（用 subgraph `volumeUSD` 作代理） | 0 |
| `min_pool_age_days` | 最小池龄（天） | 0 |
| `reserve_ratio_k` | 动态浅池阈值 `reserve_in ≥ k·amount_in`（0 = 关闭） | 0 |
| `slippage_penalty_lambda` | 流动性惩罚系数 | 0.0 |
| `refresh_interval_secs` | 热池储备刷新间隔与储备 TTL；近 5 分钟访问过的池才是热池 | 30 |
| `snapshot_refresh_secs` | 全量价格快照重建间隔 | 120 |
| `request_timeout_ms` | 单请求超时（毫秒） | 10000 |
| `pricing_mode` / `local_state` | 精算模式 / 后台同步与新鲜度策略 | `rpc` / 见示例 |
| `split` | 可选拆单的服务预算；请求仍需 `split=true` | 4 分支 / 4 候选池 / 20 段 / 1024 次 / 8000 ms |
| `token_allowlist` | 覆盖内置主流币白名单 | 空（使用内置） |

## Execution routing

`swap.quote` 固定使用 Fluxion remote 服务；`swap.build` 固定在 local runtime；`wallet.balance`、`wallet.allowance`、
`transaction.inspect`、`signature.inspect` 和 `transaction.simulate` 固定在 local runtime；`wallet.assets` 和 `wallet.collectibles` 是 local verification +
remote candidate discovery 的 hybrid capability。`wallet.connect` / `wallet.status` /
`wallet.disconnect` / `wallet.sign` / `wallet.send` 固定在 local，负责 WalletConnect session；`wallet.agent-id` 是
SPACE ID API + BSC 校验的 hybrid capability。Agent 和用户都不选择 local/remote。

| 环境变量 | 默认 | 含义 |
|---|---|---|
| `FLUXION_REMOTE_URL` | 无（必须设置） | remote 服务地址；发行版可内置默认值 |
| `FLUXION_API_KEY` | 空 | 静态 remote 认证 token；设置后不再自动注册 |
| `FLUXION_REQUIRE_AUTH` | 空 | remote server 设为 `1` 时 `/quote` 需要安装 token |
| `FLUXION_URL` | `http://127.0.0.1:8080` | 本地 runtime 地址，供 `start/status/serve` 和未来 wallet/execute |
| `FLUXION_RPC_URL` / `RPC_URL` | `https://eth.drpc.org` | 本地 Ethereum RPC；Ethereum 需显式 `--chain ethereum` |
| `ONEINCH_API_KEY` | 空 | `wallet.assets` 的候选发现；缺失时回退 registry-only |
| `FLUXION_ONEINCH_BALANCE_URL` | `https://api.1inch.com/balance/v1.2` | 可替换的 1inch Balance provider endpoint |
| `ALCHEMY_API_KEY` | 空 | `wallet.collectibles` 的 NFT 候选发现；缺失时回退 curated/explicit collections |
| `BSCSCAN_API_KEY` | 空 | `transaction.inspect` 的 BSC 合约源码验证；缺失时 `verified=null` |
| `ETHERSCAN_API_KEY` | 空 | `transaction.inspect` 的 Ethereum 合约源码验证；缺失时 `verified=null` |
| `FLUXION_BSCSCAN_API_URL` | `https://api.bscscan.com/api` | BscScan 兼容验证端点覆盖 |
| `FLUXION_ETHERSCAN_API_URL` | `https://api.etherscan.io/api` | Etherscan 兼容验证端点覆盖 |
| `FLUXION_ALCHEMY_NFT_URL` | `https://eth-mainnet.g.alchemy.com/nft/v3` | 可替换的 Alchemy NFT endpoint；BSC 自动换成 `bnb-mainnet` |
| `WALLETCONNECT_PROJECT_ID` | 空 | `wallet.connect` 的 WalletConnect Cloud project ID；缺失时返回明确错误 |
| `WALLETCONNECT_RELAY_URL` | `wss://relay.walletconnect.com` | 可替换的 WalletConnect relay endpoint |
| `FLUXION_AGENT_ID_API_URL` | `https://agentid.prd.space.id` | `wallet.agent-id` 的 SPACE ID API |
| `FLUXION_BSC_RPC_URL` | `https://bsc-dataseed.bnbchain.org` | 默认链 BSC 的本地 RPC：wallet reads、simulate、sign/send、Agent ID |
| `FLUXION_VERIFY_RPC_URL` | 空 | 可选第二 RPC；`transaction.inspect` 用它交叉校验 block hash 和 target code |
| `FLUXION_THREAT_INTEL_FILE` | 空 | 可选本地 threat-intel JSON；命中地址输出 `ADDR-007` |
| `FLUXION_THREAT_INTEL_PUBKEY` | 空 | 可选 Ed25519 公钥（32-byte hex）；设置后 threat-intel 文件必须签名 |
| `GRAPH_API_KEY` | 空 | 本地 runtime 的 discovery；服务端/开发需要 |

CLI/MCP 的 `--url` 覆盖 remote endpoint。没有配置 remote endpoint 时，`swap.quote` 返回
明确的 `RPC_ERROR`，不会回落 local。启用 `FLUXION_REQUIRE_AUTH=1` 后，local client 首次调用
会自动 `POST /register` 并保存安装 token 到 `~/.fluxion/auth.json`。

`GRAPH_API_KEY` 只放在 **gitignored 的 `.env`** 里，不进版本库。
转换实例字段为 `kind`、`contract`、`underlying`、`wrapped`、`enabled`、`wrap`、`unwrap`；
完整示例见 [config.example.yaml](../config.example.yaml)。汇率来自协议，不在 YAML 中填写固定价格。

粗选价格缓存 TTL 取 `max(refresh_interval_secs, snapshot_refresh_secs)`。全图预热使用
Multicall3 分批读取 V2 储备和 V3 `slot0`，不会将所有池标成热池；精算时才懒加载 V2 的短 TTL 储备。

