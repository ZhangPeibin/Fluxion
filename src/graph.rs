use std::collections::HashMap;

use alloy_primitives::Address;

use crate::model::{Edge, Pool};

#[derive(Debug, Default, Clone)]
pub struct Graph {
    tokens: HashMap<Address, usize>,
    token_addrs: Vec<Address>,
    pools: Vec<Pool>,
    edges: Vec<Edge>,
    out_edges: HashMap<usize, Vec<usize>>,
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    fn node_id(&mut self, token: Address) -> usize {
        if let Some(&id) = self.tokens.get(&token) {
            return id;
        }
        let id = self.token_addrs.len();
        self.token_addrs.push(token);
        self.tokens.insert(token, id);
        id
    }

    pub fn add_pool(&mut self, pool: Pool) {
        let pool_idx = self.pools.len();
        let ids: Vec<usize> = pool.coins.iter().map(|&c| self.node_id(c)).collect();
        self.pools.push(pool);
        for (i, &from) in ids.iter().enumerate() {
            for (j, _) in ids.iter().enumerate() {
                if i == j {
                    continue;
                }
                let edge_idx = self.edges.len();
                self.edges.push(Edge { pool_idx, i, j });
                self.out_edges.entry(from).or_default().push(edge_idx);
            }
        }
    }

    /// Unique tokens in insertion order, for read-only graph inspection.
    pub fn tokens(&self) -> &[Address] {
        &self.token_addrs
    }

    pub fn out_edge_indices(&self, token: Address) -> &[usize] {
        match self
            .tokens
            .get(&token)
            .and_then(|&id| self.out_edges.get(&id))
        {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }

    pub fn edge(&self, idx: usize) -> &Edge {
        &self.edges[idx]
    }

    pub fn pool(&self, idx: usize) -> &Pool {
        &self.pools[idx]
    }

    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_coin_pool_yields_two_directed_edges() {
        let a = Address::with_last_byte(1);
        let b = Address::with_last_byte(2);
        let mut g = Graph::new();
        g.add_pool(Pool {
            kind: crate::model::PoolKind::UniswapV2,
            address: Address::with_last_byte(10),
            coins: vec![a, b],
        });
        assert_eq!(g.pool_count(), 1);
        assert_eq!(g.edge_count(), 2);
        assert_eq!(g.out_edge_indices(a).len(), 1);
        assert_eq!(g.out_edge_indices(b).len(), 1);
    }

    #[test]
    fn parallel_pools_keep_parallel_edges() {
        let a = Address::with_last_byte(1);
        let b = Address::with_last_byte(2);
        let mut g = Graph::new();
        for i in 0..2 {
            g.add_pool(Pool {
                kind: crate::model::PoolKind::UniswapV2,
                address: Address::with_last_byte(20 + i),
                coins: vec![a, b],
            });
        }
        assert_eq!(g.pool_count(), 2);
        assert_eq!(g.out_edge_indices(a).len(), 2);
    }

    #[test]
    fn three_coin_pool_yields_six_directed_edges() {
        let a = Address::with_last_byte(1);
        let b = Address::with_last_byte(2);
        let c = Address::with_last_byte(3);
        let mut g = Graph::new();
        g.add_pool(Pool {
            kind: crate::model::PoolKind::Curve,
            address: Address::with_last_byte(30),
            coins: vec![a, b, c],
        });
        assert_eq!(g.edge_count(), 6);
        assert_eq!(g.out_edge_indices(a).len(), 2);
        assert_eq!(g.out_edge_indices(b).len(), 2);
        assert_eq!(g.out_edge_indices(c).len(), 2);
    }
}
