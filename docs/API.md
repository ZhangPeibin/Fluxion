# HTTP API

## `POST /quote`

请求：

```json
{
  "token_in": "0xC02a...",
  "token_out": "0xA0b8...",
  "amount_in": "1000000000000000000",
  "max_hops": 3,
  "amount_out_min": "0"
}
```

- 金额均为**十进制字符串**（`U256`）。
- `max_hops` 可选，指**边数**（默认取配置）；每次 swap、wrap、unwrap 都计一跳。
- `amount_out_min` 可选，所选总输出低于它时返回 `AmountOutBelowMin`。
- `split` 可选，默认 `false`，旧请求保留下面的单路径响应。
- `split=true` 时可传 `split_max_branches`（至少 2，且不能超过服务配置上限，默认 4）；
  不开启拆单时传分支上限返回 400。

响应（`200`）：

```json
{
  "amount_out": "2318785201",
  "price_impact_bps": 32,
  "path": [
    {
      "operation": "swap",
      "platform": "uniswap-v3",
      "pool": "0xE0554a476A092703abdB3Ef35c80e0D76d32939F",
      "i": 1, "j": 0,
      "token_in": "0xC02a...",
      "token_out": "0xA0b8...",
      "amount_in": "1000000000000000000",
      "amount_out": "2318785201"
    }
  ]
}
```

- `operation` 为 `swap` / `wrap` / `unwrap`；转换步骤的 `pool` 字段指 wrapper 合约地址。
- 返回的是报价和逐跳信息，不包含交易 calldata，也不会发送交易。

**可选多池模式：** 在同一个请求中增加 `"split": true`。响应另含 `plan_kind`、
`allocations`、`baseline_amount_out`、`search`；真实主网请求含 `quote_block`，全部精算固定在该区块。
每个 allocation 有 `amount_in`、`amount_out`、`input_share_percent` 和自己的顺序 `path`。
并行分支不会拼成顺序路径：多分支时顶层 `path` 为空，`amount_out` 是各分支输出之和。
保留原单路径时仍有一个 100% allocation；搜索预算耗尽保留最佳完整方案。
新选择的拆单/直连方案尚未计算聚合价格冲击，`price_impact_bps` 为 `null`。
完整字段和预算语义见 [多池接口说明](MULTI-POOL.md)。

## `GET /graph`

只读地导出当前路由多重图，供 Web viewer 等外部工具使用：

```json
{
  "tokens": ["0xA0b8...", "0xC02a..."],
  "edges": [
    {
      "operation": "swap",
      "from": "0xC02a...",
      "to": "0xA0b8...",
      "platform": "uniswap-v3",
      "pool": "0xE055...",
      "i": 1,
      "j": 0,
      "mid_price": 2.3187e-9,
      "reserve": "1234567890123456789"
    }
  ]
}
```

- 每条边是一个有向交换或转换步骤；`i` / `j` 是 `Pool.coins` 中的币索引。
- `mid_price` 是输出 raw units / 输入 raw units 的比值；换算整币价格需要结合 token decimals。
- `mid_price` / `reserve` 来自当前价格快照；没有快照或该边数据不可用时**省略这两个字段**（不报错）。
- 协议转换边省略 `reserve`，避免把内部哨兵值显示成真实流动性。
- 只读：不触发新快照、不消耗限流额度，也不影响 `/quote`。

## `GET /info`

返回 `mode`（`mainnet` / `mock`）、生效配置、图规模和 `platform_edges`。配置不包含 RPC URL；
`pools` 是图内记录数，也包含 wrapper link。基准工具用它确认实例模式与实际加载的平台。

## `GET /health` / `GET /ready`

- `/health`：进程存活。
- `/ready`：默认对上游 RPC 做一次 `eth_blockNumber`，不通返回 `503`；local 模式只检查快照是否可服务。

## `GET /state`

local 模式返回快照区块/hash、年龄、相对最后观测链头的落后、池覆盖、同步 RPC/耗时、
最近错误和重组次数。成功报价附加 `state` 元数据及 `quote_block`；Viewer 显示模式、
区块、年龄和落后。冷启动、过期、不完整或不支持的本地状态返回明确的 503，不回退到请求内 RPC。
字段、策略与首阶段支持范围见 [LOCAL-STATE](LOCAL-STATE.md)。

## 错误

`QuoteError` → HTTP 状态：`NoPathFound`(404)、`AmountOutBelowMin`/`InsufficientLiquidity`/
`Overflow`(422)、`ZeroAmount`(400)、`PoolNotFound`(404)、`RpcError`(502)、限流(429)。

