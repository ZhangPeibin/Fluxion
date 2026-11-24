//! Print the `site/` capability dataset derived from `capabilities/catalog.yaml`.
//!
//! ```text
//! cargo run -p eollo-protocol --bin catalog-json > site/src/data/capabilities.json
//! ```

use eollo_protocol::capability::catalog::load_embedded;

fn main() {
    let catalog = load_embedded();
    let json = catalog.site_catalog_json();
    println!(
        "{}",
        serde_json::to_string_pretty(&json).expect("catalog serializes to JSON")
    );
}
