use std::sync::{atomic::AtomicBool, Arc};

use sqlx::SqlitePool;
use tokio::sync::broadcast;

use crate::models::DomainEvent;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub event_tx: broadcast::Sender<DomainEvent>,
    pub demo_running: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            pool,
            event_tx,
            demo_running: Arc::new(AtomicBool::new(false)),
        }
    }
}
