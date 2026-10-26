# P0 Rust 热身

示例代码：`examples/p0_rust_warmup.rs`（`cargo run --example p0_rust_warmup`）

## 讲了什么

用一个「报价源」的简化版，把本项目 P1 起会用到的 Rust 基础串起来：

### 1. 所有权 / 借用（ownership & borrowing）

- `String` 是**拥有所有权**的字符串（存在堆上）；`&str` / `&Token` 是**借用**（不拿走）。
- 函数参数用 `&Token` 表示「我借用一下，用完还你」，调用方仍持有所有权。
- `"ETH"` 是编译期常量 `&'static str`；`.to_string()` 把它转成拥有所有权的 `String`。
- 规则一句话：**同一时刻，一个值要么有多个只读借用（`&`），要么有一个可变借用（`&mut`），二者不能同时**。

### 2. trait（接口）

- `trait QuoteSource { async fn amount_out(...) }` 定义一份「契约」。
- `impl QuoteSource for UniswapV2` / `for FixedRate` 各自实现 —— 这就是多态。
- 好处：上层代码（`query` / `main`）只依赖 `QuoteSource`，不关心具体是哪个平台。

### 3. Result 与 `?`

- `Result<T, E>` 表示「成功返回 T，失败返回 E」。
- `?` 运算符：值 `Ok` 就取出里面的 T 继续；值 `Err` 就立刻把错误**向上返回**（提前退出）。
- 例：`query(...).await?` —— 失败则 `main` 直接返回错误。

### 4. async/await + tokio

- `async fn` 返回一个 Future；`.await` 才真正执行。
- `#[tokio::main]` 把 `main` 包装进 tokio 异步运行时（否则没有执行 Future 的环境）。
- 为什么本项目要 async：询价要打 RPC（网络 I/O），异步能并发等，不阻塞线程。

### 5. 检查点核心：async fn 为何让 trait 不「dyn 兼容」

**现象**：不加 `#[async_trait]` 时，`Vec<Box<dyn QuoteSource>>` 编译报错 `E0038`：
「the trait `QuoteSource` is not dyn compatible ... because method `amount_out` is `async`」。

**原因**：
- `async fn amount_out(...)` 会被编译器**脱糖**成「返回一个 `impl Future` 的普通函数」。
- 这个 Future 是**编译器为每个 impl 单独生成的具体类型**，`UniswapV2` 和 `FixedRate` 的
  Future 类型**不一样**，且大小未知。
- 而 `dyn QuoteSource`（trait object）靠一个 **vtable（虚表）** 在运行时派发，vtable 里
  每个方法必须有**固定的签名**。但这里没有单一、已知大小的 Future 类型可写进 vtable。

**解决**：把 Future **装箱到堆上**，变成固定大小的指针 `Box<dyn Future<...> + Send>`，
vtable 里就能存下这个指针类型了。

- `#[async_trait]` 宏就是**自动做了这件事**：把 `async fn` 重写成返回
  `Pin<Box<dyn Future<Output = ...> + Send + 'async_trait>>` 的普通函数。
- 也可以手写等价形式（不引入宏）：
  ```rust
  fn amount_out(&self, ...) -> Pin<Box<dyn Future<Output = Result<u128, Box<dyn Error>>> + Send + '_>>;
  ```
  但很啰嗦，所以生态里普遍用 `async_trait`。

**注意**：`#[async_trait]` 必须同时加在 **trait 定义** 和 **每个 impl 块** 上。

## 你的要点（待补充）

（在这里写下你自己的理解 / 疑问）
