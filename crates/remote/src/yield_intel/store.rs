//! Warm snapshot store for yield discovery.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use eollo_protocol::yield_capability::{
    YieldDiscoverRequest, YieldDiscoverResponse, YieldHistoryRequest, YieldHistoryResponse,
};

use super::Registry;
use super::YieldError;
use super::history;
use super::provider::{HistoryPoint, YieldProvider, now};
use super::score::{YieldSnapshot, build_snapshot, discover};

#[derive(Debug, Clone)]
pub struct YieldOptions {
    pub refresh_secs: u64,
    pub max_age_secs: u64,
    pub hard_max_age_secs: u64,
}

impl Default for YieldOptions {
    fn default() -> Self {
        Self {
            refresh_secs: 300,
            max_age_secs: 900,
            hard_max_age_secs: 3600,
        }
    }
}

pub struct YieldStore {
    registry: Registry,
    provider: YieldProvider,
    options: YieldOptions,
    snapshot: RwLock<Option<Arc<YieldSnapshot>>>,
    history: RwLock<HashMap<String, CachedHistory>>,
}

#[derive(Debug, Clone)]
struct CachedHistory {
    fetched_at: u64,
    points: Vec<HistoryPoint>,
}

impl YieldStore {
    pub fn new(registry: Registry, provider: YieldProvider, options: YieldOptions) -> Arc<Self> {
        Arc::new(Self {
            registry,
            provider,
            options,
            snapshot: RwLock::new(None),
            history: RwLock::new(HashMap::new()),
        })
    }

    pub fn with_snapshot(snapshot: YieldSnapshot) -> Arc<Self> {
        let registry = Registry::load_default().expect("default registry");
        let provider = YieldProvider::new(crate::rpc::RpcClient::new(String::new()));
        Arc::new(Self {
            registry,
            provider,
            options: YieldOptions::default(),
            snapshot: RwLock::new(Some(Arc::new(snapshot))),
            history: RwLock::new(HashMap::new()),
        })
    }

    pub fn start_background(self: &Arc<Self>) {
        let store = self.clone();
        tokio::spawn(async move {
            if let Err(error) = store.refresh().await {
                tracing::warn!(%error, "yield snapshot warmup failed");
            }
            let interval = Duration::from_secs(store.options.refresh_secs.max(1));
            loop {
                tokio::time::sleep(interval).await;
                if let Err(error) = store.refresh().await {
                    tracing::warn!(%error, "yield snapshot refresh failed");
                }
            }
        });
    }

    pub async fn refresh(&self) -> Result<(), YieldError> {
        let data = self.provider.fetch().await?;
        let checks = futures::future::join_all(
            self.registry
                .entries()
                .iter()
                .map(|entry| async move { (entry.id.clone(), self.provider.verify(entry).await) }),
        )
        .await
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
        let snapshot = build_snapshot(
            self.registry.entries(),
            &data.pools,
            &data.protocol_meta,
            &checks,
            now(),
            data.sources,
            data.warnings,
        );
        tracing::info!(
            opportunities = snapshot.opportunities.len(),
            excluded = snapshot.excluded.len(),
            "yield snapshot published"
        );
        *self.snapshot.write().unwrap() = Some(Arc::new(snapshot));
        Ok(())
    }

    pub fn snapshot(&self) -> Option<Arc<YieldSnapshot>> {
        self.snapshot.read().unwrap().clone()
    }

    pub fn discover(
        &self,
        request: &YieldDiscoverRequest,
    ) -> Result<YieldDiscoverResponse, YieldError> {
        let snapshot = self.snapshot().ok_or(YieldError::Stale)?;
        discover(
            &snapshot,
            request,
            now(),
            self.options.max_age_secs,
            self.options.hard_max_age_secs,
        )
    }

    pub async fn history(
        &self,
        request: &YieldHistoryRequest,
    ) -> Result<YieldHistoryResponse, YieldError> {
        if request.days == 0 || request.days > 365 {
            return Err(YieldError::Invalid(
                "days must be between 1 and 365".to_string(),
            ));
        }
        if request.max_points == 0 || request.max_points > 2000 {
            return Err(YieldError::Invalid(
                "max_points must be between 1 and 2000".to_string(),
            ));
        }
        let entry = self
            .registry
            .entries()
            .iter()
            .find(|entry| entry.id == request.opportunity_id)
            .ok_or_else(|| {
                YieldError::Invalid(format!(
                    "unknown opportunity_id: {}",
                    request.opportunity_id
                ))
            })?
            .clone();
        let pool_id = entry.defillama_pool.clone().ok_or_else(|| {
            YieldError::Unsupported(format!("{} has no DefiLlama pool id", entry.id))
        })?;
        let now = now();
        let cached = self.history.read().unwrap().get(&entry.id).cloned();
        let (fetched_at, points) = match cached {
            Some(cached) if now.saturating_sub(cached.fetched_at) <= self.options.refresh_secs => {
                (cached.fetched_at, cached.points)
            }
            _ => {
                let points = self.provider.fetch_history(&pool_id).await?;
                self.history.write().unwrap().insert(
                    entry.id.clone(),
                    CachedHistory {
                        fetched_at: now,
                        points: points.clone(),
                    },
                );
                (now, points)
            }
        };
        history::build_response(&entry, points, request, fetched_at, now)
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn options(&self) -> &YieldOptions {
        &self.options
    }
}
