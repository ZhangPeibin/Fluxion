# Agent examples

Natural-language prompts for an agent connected to Eollo over MCP. Each row maps a prompt to the
capability the agent should call. Read-only examples need no wallet. Wallet examples need a
WalletConnect session and a reachable relay.

See [MCP](MCP.md) for client setup and [capabilities](capabilities/README.md) for contracts.

## Read-only

| Job | 中文 | English | Tool | Chain | Verified |
|---|---|---|---|---|---|
| Balances | 查一下这个 BSC 地址的 BNB、USDT 和 CAKE 余额：0x8894E0a0c962CB723c1976a4421c95949bE2D4E3 | Show the BNB, USDT and CAKE balances for this BSC address: 0x8894E0a0c962CB723c1976a4421c95949bE2D4E3 | `wallet_balance` | bsc | Yes |
| Assets | 列出这个 BSC 地址持有的代币，只查内置名单，并告诉我有多少无法识别。 | List the tokens this BSC address holds, using the built-in registry, and tell me how many are unknown. | `wallet_assets` | bsc | Yes |
| Allowance | 查一下这个 BSC 地址给 PancakeSwap V2 router 的 USDT 和 CAKE 授权额度。 | Check the USDT and CAKE allowances this BSC address granted to the PancakeSwap V2 router. | `wallet_allowance` | bsc | Yes |
| Quote | 在 BSC 上把 1 个 BNB 换成 USDT，给我最优报价、路线和价格影响。 | Swap 1 BNB to USDT on BSC and show the best quote, route and price impact. | `swap_quote` | bsc | Yes |
| Build | 把刚才的报价构建成可签名的 calldata，收款地址 0x8894E0a0c962CB723c1976a4421c95949bE2D4E3，滑点 0.5%。 | Build signable calldata from that quote for recipient 0x8894E0a0c962CB723c1976a4421c95949bE2D4E3 at 0.5% slippage. | `swap_build` | bsc | Yes |
| Simulate | 模拟这笔 BSC 交易，告诉我是否 revert，估算 gas。 | Simulate this BSC transaction, tell me if it reverts, and estimate gas. | `transaction_simulate` | bsc | Yes |
| Inspect | 签名前检查这笔 calldata：它在做什么、会动哪些资产、有没有 delegatecall 风险？ | Inspect this calldata before I sign: what does it do, which assets move, and is there a delegatecall risk? | `transaction_inspect` | bsc | Yes |
| Verify | 验证这笔 BSC 交易是否成功，实际转了什么：0x08aac62c51a41b2c190ee1e466bd219b1c14fdd4a534179ca8c24795b3de4e6e | Verify this BSC transaction: did it succeed, and what actually moved? 0x08aac62c51a41b2c190ee1e466bd219b1c14fdd4a534179ca8c24795b3de4e6e | `transaction_verify` | bsc | Yes |
| Permit2 | 检查这份 Permit2 请求：spender、额度、过期时间和链 ID 是否正确？ | Inspect this Permit2 request: are the spender, amount, expiration and chain ID correct? | `signature_inspect` | bsc | Yes |
| Yield discover | 我在 BSC 上有 10 个 BNB，找风险中等、收益不错的借贷机会，说明退出条件。 | I have 10 BNB on BSC. Find a lending market with a reasonable APY and explain the exit conditions. | `yield_discover` | bsc | Yes |
| Yield build | 帮我生成把 1 个 BNB 存进 Venus vBNB 的 calldata 和授权步骤。 | Build the calldata and approval steps to deposit 1 BNB into Venus vBNB. | `yield_build` | bsc | Yes |
| Yield history | 看一下 Venus vBNB 过去 30 天的 APY 变化和 TVL。 | Show the last 30 days of Venus vBNB APY and TVL. | `yield_history` | bsc | Yes |
| Yield withdraw | 帮我生成把 100 个 vBNB 从 Venus 赎回成 BNB 的 calldata。 | Build calldata to redeem 100 vBNB from Venus into BNB. | `yield_withdraw` | bsc | Yes |
| Position | 看一下这个地址在 BSC 上的 Venus 仓位，值多少 BNB，APY 是多少：0x4d2e4add7bbed906e949954516bb735cd51a5dca | Show this address's Venus position on BSC: what is it worth in BNB, and what is the APY? 0x4d2e4add7bbed906e949954516bb735cd51a5dca | `position_analyze` | bsc | Yes |
| Agent ID | 解析 alice.ai 这个 BSC Agent ID，给我 owner 地址和 token id。 | Resolve the BSC Agent ID alice.ai: give me the owner and token id. | `wallet_agent_id` | bsc | Yes |
| NFT coverage | 查一下这个地址在 BSC 上持有的 NFT；没有 indexer 就告诉我覆盖范围。 | Show the NFTs this address holds on BSC; if there is no indexer, tell me the coverage. | `wallet_collectibles` | bsc | Yes, empty without an indexer |
| Four.meme | 看看最近的 Four.meme Agent ID 专属代币，哪个流动性最好。 | Show recent Four.meme Agent ID Exclusive tokens and which has the best liquidity. | `fourmeme_agent_tokens` and `fourmeme_token` | bsc | Partial, no recent tokens found |

## Wallet required

These need a WalletConnect session. The machine that runs Eollo must be able to reach
`relay.walletconnect.com`.

| Job | 中文 | English | Tool | Chain | Verified |
|---|---|---|---|---|---|
| Connect | 连接我的 BSC 钱包，先显示二维码，再轮询连接状态。 | Connect my BSC wallet, show the QR first, then poll the session status. | `wallet_connect` and `wallet_status` | bsc | Needs relay |
| Sign | 用我的钱包签这条消息：hello。 | Sign this message with my wallet: hello. | `wallet_sign` | bsc | Needs wallet |
| Send | 给我这个地址转 0.01 BNB，先模拟，失败就不要发。 | Send 0.01 BNB to this address; simulate first and do not send if it fails. | `wallet_send` | bsc | Needs wallet |
| Send calldata | 检查并发送这笔 BSC 交易，签名前先做策略检查。 | Inspect and send this BSC transaction; run the policy check before signing. | `transaction_send` | bsc | Needs wallet |

## Test addresses

- `0x8894E0a0c962CB723c1976a4421c95949bE2D4E3`: public BSC hot wallet with BNB, USDT, USDC and
  CAKE balances. Read-only.
- `0x4d2e4add7bbed906e949954516bb735cd51a5dca`: public address with a Venus vBNB position.
  Read-only.
- `alice.ai`: BSC Agent ID example.

The read-only examples do not move funds. Do not use these addresses as recipients for real
transfers.

## Notes

- BSC swap quotes use PancakeSwap V2 and V3. `swap_build` rejects split and mixed V2/V3 routes;
  re-quote with a single route.
- BSC yield discovery needs `asset: BNB`. The conservative profile may exclude Venus because its
  safety tier is `established`.
- `yield_discover` on Ethereum uses ETH. Example: "I have 10 ETH. Find the safest yield at a
  reasonable APY under conservative."
- `transaction.inspect` and `signature.inspect` are advisory. They do not sign or guarantee safety.
