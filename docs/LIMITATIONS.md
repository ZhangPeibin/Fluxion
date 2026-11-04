# Limitations

- 仅 Ethereum 主网；exact-in；默认单路径，可选独立直连池或互不共享池的多跳路径拆单；不保证全市场最优。
- 多池分配有候选、网格和调用预算限制；支持互不共享池的多跳路径拆单，不支持共享池状态联动。
- 原生 ETH 不支持；ETH↔WETH 包装由调用方处理。stETH↔wstETH 是独立的、已支持的协议转换。
- Curve 适配器同时支持 stableswap（`get_dy(int128,…)`）与 crypto（`get_dy(uint256,…)`）；
  **发现**默认覆盖主 stableswap + crypto 注册表（`curve_registry_ids: [0, 5]`）；
  factory/metapool 可通过把 `7`（MetaRegistry）加入 `curve_registry_ids` 纳入，但会显著变慢。
- 公共 RPC 可能限流；正常请求的粗选在内存执行，主要等待来自候选精算。增加候选能力组会增加
  RPC 开销；延迟参考 [benchmark](benchmarks/README.md)，不提供固定 SLA。
- 不通用支持 fee-on-transfer 或任意 rebasing token；stETH 转换有专用适配器，不意味着其他特殊 token 自动可用。
- 默认单路径的快照、V2 缓存和实时调用可能来自不同区块；多池精算固定区块，但不构建或模拟交易。
- 公开 HTTP 无鉴权，仅简单限流。

