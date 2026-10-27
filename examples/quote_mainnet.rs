use std::sync::Arc;

use alloy_primitives::{Address, U256};

use fluxion::adapters::{RpcV2Reserves, UniswapV2};
use fluxion::config::Config;
use fluxion::discovery;
use fluxion::graph::Graph;
use fluxion::model::QuoteRequest;
use fluxion::router::{Router, RouterConfig};

fn addr(s: &str) -> Address {
    s.parse().unwrap()
}

const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let config = Config::from_file("config.yaml").unwrap_or_default();
    let key = std::env::var("GRAPH_API_KEY").expect("GRAPH_API_KEY not set");

    println!("discovering pools...");
    let pools = discovery::discover_v2(&config, &key).await.unwrap();
    println!("discovered {} pools", pools.len());

    let mut graph = Graph::new();
    for pool in &pools {
        graph.add_pool(pool.clone());
    }

    let source = UniswapV2::new(Arc::new(RpcV2Reserves::new(config.rpc_url.clone())));
    let router = Router::new(RouterConfig {
        max_hops: config.max_hops,
        top_k: config.top_k,
        lambda: config.slippage_penalty_lambda,
    });

    let req = QuoteRequest {
        token_in: addr(WETH),
        token_out: addr(USDC),
        amount_in: U256::from(1_000_000_000_000_000_000u128),
        max_hops: Some(2),
        amount_out_min: None,
    };
    match router.route(&graph, &source, &req).await {
        Ok(route) => {
            println!("amount_out = {}", route.amount_out);
            for hop in &route.hops {
                println!(
                    "  {} -> {} via {}",
                    hop.token_in, hop.token_out, hop.pool.address
                );
            }
        }
        Err(e) => println!("error: {e}"),
    }
}
