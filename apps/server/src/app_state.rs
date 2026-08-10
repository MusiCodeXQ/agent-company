use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Notify};

use crate::models::DomainEvent;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub event_tx: broadcast::Sender<DomainEvent>,
    pub worker_notify: Arc<Notify>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            pool,
            event_tx,
            worker_notify: Arc::new(Notify::new()),
        }
    }
}
