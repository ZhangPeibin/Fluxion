use std::error::Error;

use async_trait::async_trait;

// 1) 一个 Token：String 是「拥有所有权」的类型
#[derive(Debug, Clone)]
struct Token {
    symbol: String,
    decimals: u8,
}

// 2) 报价源 trait —— 这就是本项目真正的 QuoteSource 的简化版
//    #[async_trait] 宏：把 async fn 变成返回 BoxFuture 的普通 fn，使其 dyn 兼容
#[async_trait]
trait QuoteSource {
    async fn amount_out(&self, token_in: &Token, amount_in: u128) -> Result<u128, Box<dyn Error>>;
}

// 3) 实现一：Uniswap V2 常数积 AMM
struct UniswapV2 {
    reserve_in: u128,
    reserve_out: u128,
}

#[async_trait]
impl QuoteSource for UniswapV2 {
    async fn amount_out(&self, _token_in: &Token, amount_in: u128) -> Result<u128, Box<dyn Error>> {
        // x*y=k：amount_out = amount_in * 997 * r_out / (r_in * 1000 + amount_in * 997)
        let num = amount_in * 997 * self.reserve_out;
        let den = self.reserve_in * 1000 + amount_in * 997;
        Ok(num / den)
    }
}

// 4) 实现二：固定汇率（用来演示 trait 的多实现 / 平行边）
struct FixedRate {
    rate: u128,
}

#[async_trait]
impl QuoteSource for FixedRate {
    async fn amount_out(&self, _token_in: &Token, amount_in: u128) -> Result<u128, Box<dyn Error>> {
        Ok(amount_in * self.rate)
    }
}

// 5) 一个接收 trait object 的函数（&dyn QuoteSource）
async fn query(source: &dyn QuoteSource, token_in: &Token, amount_in: u128) -> Result<u128, Box<dyn Error>> {
    source.amount_out(token_in, amount_in).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let amount_in: u128 = 1000;
    let eth = Token { symbol: "ETH".to_string(), decimals: 18 };
    println!("询价 {amount_in} 个 {} ({} 位小数):", eth.symbol, eth.decimals);

    // 直接传具体类型
    let v2 = UniswapV2 { reserve_in: 1_000_000, reserve_out: 3_000_000 };
    let out = query(&v2, &eth, amount_in).await?;
    println!("V2 单跳 amount_out = {out}");
    // 把不同实现放进同一个 Vec —— 必须 Box<dyn QuoteSource>
    let sources: Vec<Box<dyn QuoteSource>> = vec![
        Box::new(UniswapV2 { reserve_in: 1_000_000, reserve_out: 3_000_000 }),
        Box::new(FixedRate { rate: 2 }),
    ];

    // 遍历所有「平行边」，取最优 —— 这就是询价选最优的雏形
    let mut best = 0u128;
    for s in &sources {
        let o = s.amount_out(&eth, amount_in).await?;
        println!("  候选 = {o}");
        if o > best {
            best = o;
        }
    }
    println!("最优 amount_out = {best}");

    Ok(())
}