use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, Mutex, Notify};

use crate::{
    container_engine::{ContainerEngine, DockerCliEngine},
    models::DomainEvent,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub event_tx: broadcast::Sender<DomainEvent>,
    pub worker_notify: Arc<Notify>,
    pub container_engine: Arc<dyn ContainerEngine>,
    pub container_lock: Arc<Mutex<()>>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            pool,
            event_tx,
            worker_notify: Arc::new(Notify::new()),
            container_engine: Arc::new(DockerCliEngine::from_env()),
            container_lock: Arc::new(Mutex::new(())),
        }
    }
}
