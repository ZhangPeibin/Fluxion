use std::time::Duration;

use alloy_primitives::{Address, U256};

use fluxion::adapters::V2Reserves;
use fluxion::rpc::RpcClient;
use fluxion::state::StateCache;

fn addr(s: &str) -> Address {
    s.parse().unwrap()
}

#[tokio::test]
#[ignore]
async fn multicall3_hot_refresh() {
    let cache = StateCache::new(
        RpcClient::new("https://eth.drpc.org".to_string()),
        Duration::from_secs(5),
    );
    let pool_a = addr("0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc");
    let pool_b = addr("0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852");

    // Lazy-load marks the pools hot.
    let ra = cache.get_reserves(pool_a).await.unwrap();
    let rb = cache.get_reserves(pool_b).await.unwrap();
    assert!(ra[0] > U256::ZERO);
    assert!(rb[0] > U256::ZERO);

    // Batch refresh via Multicall3.
    cache.refresh_hot().await.unwrap();

    let ra2 = cache.get_reserves(pool_a).await.unwrap();
    let rb2 = cache.get_reserves(pool_b).await.unwrap();
    assert!(ra2[0] > U256::ZERO);
    assert!(rb2[0] > U256::ZERO);
}
