//! Export the running service's graph as Mermaid and interactive HTML.
//! Uses the same discovered universe and filtering as `/quote`.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use alloy_primitives::Address;
use serde::Deserialize;

#[derive(Deserialize)]
struct Graph {
    tokens: Vec<Address>,
    edges: Vec<Edge>,
}

#[derive(Deserialize)]
struct Edge {
    from: Address,
    to: Address,
    platform: String,
    pool: Address,
}

fn short(a: Address) -> String {
    let s = format!("{a:#x}");
    format!("{}…{}", &s[..6], &s[s.len() - 4..])
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = "http://127.0.0.1:8080".to_string();
    let mut token: Option<Address> = None;
    let mut depth = 2usize;
    let mut all = false;
    let mut out = "graph".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--service" => service = args.next().ok_or("--service needs a URL")?,
            "--token" => token = Some(args.next().ok_or("--token needs an address")?.parse()?),
            "--depth" => depth = args.next().ok_or("--depth needs an integer")?.parse()?,
            "--out" => out = args.next().ok_or("--out needs a prefix")?,
            "--all" => all = true,
            "--help" | "-h" => {
                println!(
                    "Usage: cargo run --bin graph -- [--service URL] [--token ADDRESS] [--depth N] [--all] [--out PREFIX]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {flag}").into()),
        }
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let graph: Graph = client
        .get(format!("{}/graph", service.trim_end_matches('/')))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if token.is_some_and(|token| !graph.tokens.contains(&token)) {
        return Err("focus token is absent from the service graph".into());
    }
    let mut selected: HashSet<Address> = HashSet::new();
    if let Some(start) = token.filter(|_| !all) {
        selected.insert(start);
        let mut frontier = vec![start];
        for _ in 0..depth {
            let mut next = Vec::new();
            for edge in &graph.edges {
                if frontier.contains(&edge.from) && selected.insert(edge.to) {
                    next.push(edge.to);
                }
            }
            frontier = next;
        }
    } else {
        selected.extend(graph.tokens);
    }
    let mut ordered: Vec<Address> = selected.iter().copied().collect();
    ordered.sort();
    let ids: HashMap<Address, usize> = ordered.iter().enumerate().map(|(i, a)| (*a, i)).collect();
    let edges: Vec<&Edge> = graph
        .edges
        .iter()
        .filter(|e| selected.contains(&e.from) && selected.contains(&e.to))
        .collect();
    let mut mermaid = "flowchart LR\n".to_string();
    let nodes: Vec<_> = ordered
        .iter()
        .map(|a| {
            mermaid.push_str(&format!("  t{}[\"{}\"]\n", ids[a], short(*a)));
            serde_json::json!({ "id": ids[a], "label": short(*a), "title": format!("{a:#x}") })
        })
        .collect();
    let links: Vec<_> = edges.iter().map(|e| {
        mermaid.push_str(&format!("  t{} -->|\"{} {}\"| t{}\n", ids[&e.from], e.platform, short(e.pool), ids[&e.to]));
        serde_json::json!({ "from": ids[&e.from], "to": ids[&e.to], "label": format!("{} {}", e.platform, short(e.pool)), "arrows": "to" })
    }).collect();
    // Escape '<' so even labels from another service cannot close the script tag.
    let nodes = serde_json::to_string(&nodes)?.replace('<', "\\u003c");
    let links = serde_json::to_string(&links)?.replace('<', "\\u003c");
    let html = format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>Fluxion graph</title>
<script src="https://unpkg.com/vis-network/standalone/umd/vis-network.min.js"></script>
<style>body{{margin:0}}#net{{width:100vw;height:100vh}}</style></head><body><div id="net"></div>
<script>const nodes=new vis.DataSet({nodes});const edges=new vis.DataSet({links});
new vis.Network(document.getElementById('net'),{{nodes,edges}},{{nodes:{{shape:'dot'}},interaction:{{hover:true}}}});
</script></body></html>"#
    );
    if let Some(parent) = Path::new(&out)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(format!("{out}.mmd"), mermaid)?;
    std::fs::write(format!("{out}.html"), html)?;
    println!(
        "{} tokens, {} edges; wrote {out}.mmd and {out}.html",
        ordered.len(),
        edges.len()
    );
    Ok(())
}
