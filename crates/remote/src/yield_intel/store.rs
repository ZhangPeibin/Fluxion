//! Warm snapshot store for yield discovery.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use fluxion_protocol::yield_capability::{YieldDiscoverRequest, YieldDiscoverResponse};

use super::YieldError;
use super::provider::{YieldProvider, now};
use super::registry::Registry;
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
}

impl YieldStore {
    pub fn new(registry: Registry, provider: YieldProvider, options: YieldOptions) -> Arc<Self> {
        Arc::new(Self {
            registry,
            provider,
            options,
            snapshot: RwLock::new(None),
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

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn options(&self) -> &YieldOptions {
        &self.options
    }
}
