# Graph viewer

`ui/` 是一个独立的 React + Vite + vis-network 前端，只读地轮询 `GET /graph` 查看整张多重图，
并可试算路由。

```bash
# 终端 1：服务（默认 :8080）
cargo run -p fluxion-remote -- serve --config config.example.yaml

# 终端 2：前端（默认 :5173）；首次需安装依赖
cd ui
npm install
npm run dev
```

打开 <http://localhost:5173>。Vite dev server 把 `/api` 代理到 `http://localhost:8080` 并去掉
`/api` 前缀，浏览器保持同源，服务端无需 CORS。

- 渲染全部 token（节点）与有向池跳（边）；节点优先显示常见 token 符号（USDC/WETH/…，未知 token
  回退为短地址），边标注平台 + pool（默认关闭，可在侧栏打开），平行边按方向和序号弯折区分。
- 平台图例兼作筛选器，并显示各平台边数；另有 token 搜索（淡出无关节点/边）与最小流动性
  （raw reserve）过滤。
- ShibaSwap 与 wstETH 转换有独立图例；转换边用虚线表示，不参与池子储备阈值过滤。
- 悬停显示完整地址、pool、platform、`mid_price`、`reserve`；点击节点/边打开详情面板（含 Etherscan 链接）。
- From/To 可点击节点或直接输入地址，金额默认填入 **1 个整 token**（按 token decimals 换算为 raw units，
  也可手动改成任意 raw 数值）；调用 `POST /quote` 后高亮最优路由、
   其余边淡出，并显示 `amount_out`、`price_impact_bps` 与逐跳明细。
- 默认勾选 Multi-pool routing，可切回单路径；多池显示分支比例、输入/输出和报价区块，
  同时高亮所有分支，未计算的聚合价格冲击显示为 “—”。
- 定时刷新（默认 5s，可调）；增量更新数据，保留节点位置、选择与路由高亮。

> 节点符号来自 `ui/src/graph.js` 的内置主网 token 表（覆盖默认 allowlist）；自定义 token 可自行补充。
> 生产打包暂缓：v1 只支持上述双进程开发模式。`npm run build` 会产出静态资源，但尚未由服务托管或部署。

