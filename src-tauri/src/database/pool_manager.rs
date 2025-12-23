//! Connection Pool Manager
//!
//! Manages persistent database connections with caching per connection UUID.
//! Provides health checks, auto-reconnect, and connection status tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use super::clickhouse::ClickhouseDriver;
use super::postgres::PostgresDriver;
use super::redis::RedisDriver;
use super::sqlite::SqliteDriver;
use super::{
    ClickhouseConfig, ClickhouseProtocol, DatabaseDriver, PostgresConfig, RedisConfig, SqliteConfig,
};
use crate::db::models::TestConnectionResult;

/// Connection status enum
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting,
}

/// Configuration needed to create a driver
#[derive(Clone, Debug)]
pub struct ConnectionConfig {
    pub db_type: String,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub database: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl: Option<bool>,
    pub file_path: Option<String>,
}

/// Entry in the connection pool
struct PoolEntry {
    driver: Arc<Box<dyn DatabaseDriver>>,
    config: ConnectionConfig,
    status: ConnectionStatus,
    last_used: Instant,
    last_error: Option<String>,
}

/// Connection pool manager
pub struct PoolManager {
    pools: RwLock<HashMap<String, PoolEntry>>,
}

impl Default for PoolManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PoolManager {
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
        }
    }

    /// Create a driver from configuration
    fn create_driver(config: &ConnectionConfig) -> Result<Box<dyn DatabaseDriver>, String> {
        match config.db_type.as_str() {
            "postgres" | "postgresql" => {
                let pg_config = PostgresConfig {
                    host: config.host.clone().unwrap_or_default(),
                    port: config.port.unwrap_or(5432),
                    database: config.database.clone().unwrap_or_default(),
                    username: config.username.clone().unwrap_or_default(),
                    password: config.password.clone().unwrap_or_default(),
                    ssl: config.ssl.unwrap_or(false),
                };
                Ok(Box::new(PostgresDriver::new(pg_config)))
            }
            "sqlite" | "sqlite3" => {
                let path = config
                    .file_path
                    .clone()
                    .ok_or("File path is required for SQLite connections")?;
                let sqlite_config = SqliteConfig { file_path: path };
                Ok(Box::new(SqliteDriver::new(sqlite_config)))
            }
            "redis" => {
                let redis_config = RedisConfig {
                    host: config.host.clone().unwrap_or_default(),
                    port: config.port.unwrap_or(6379),
                    password: config.password.clone(),
                    db: config.database.clone().and_then(|d| d.parse().ok()),
                    tls: config.ssl.unwrap_or(false),
                };
                Ok(Box::new(RedisDriver::new(redis_config)))
            }
            "clickhouse" => {
                let ch_config = ClickhouseConfig {
                    host: config
                        .host
                        .clone()
                        .unwrap_or_else(|| "localhost".to_string()),
                    port: config.port.unwrap_or(8123),
                    database: config
                        .database
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    username: config
                        .username
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    password: config.password.clone().unwrap_or_default(),
                    protocol: ClickhouseProtocol::Http,
                    ssl: config.ssl.unwrap_or(false),
                };
                Ok(Box::new(ClickhouseDriver::new(ch_config)))
            }
            _ => Err(format!("Unsupported database type: {}", config.db_type)),
        }
    }

    /// Get or create a connection for the given UUID
    pub async fn get_connection(
        &self,
        uuid: &str,
        config: ConnectionConfig,
    ) -> Result<Arc<Box<dyn DatabaseDriver>>, String> {
        // Check if we have an existing connected pool
        {
            let pools = self.pools.read().await;
            if let Some(entry) = pools.get(uuid) {
                if entry.status == ConnectionStatus::Connected {
                    return Ok(entry.driver.clone());
                }
            }
        }

        // Need to create or reconnect
        self.connect(uuid, config).await
    }

    /// Explicitly connect (or reconnect) a connection
    pub async fn connect(
        &self,
        uuid: &str,
        config: ConnectionConfig,
    ) -> Result<Arc<Box<dyn DatabaseDriver>>, String> {
        // Update status to reconnecting if entry exists
        {
            let mut pools = self.pools.write().await;
            if let Some(entry) = pools.get_mut(uuid) {
                entry.status = ConnectionStatus::Reconnecting;
            }
        }

        // Create new driver
        let driver = Self::create_driver(&config)?;
        let driver = Arc::new(driver);

        // Test the connection
        let test_result = driver.test_connection().await?;

        let status = if test_result.success {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::Disconnected
        };

        let entry = PoolEntry {
            driver: driver.clone(),
            config,
            status: status.clone(),
            last_used: Instant::now(),
            last_error: if test_result.success {
                None
            } else {
                Some(test_result.message.clone())
            },
        };

        // Store in pool
        {
            let mut pools = self.pools.write().await;
            pools.insert(uuid.to_string(), entry);
        }

        if status == ConnectionStatus::Connected {
            Ok(driver)
        } else {
            Err(test_result.message)
        }
    }

    /// Disconnect and remove a connection from the pool
    pub async fn disconnect(&self, uuid: &str) {
        let mut pools = self.pools.write().await;
        pools.remove(uuid);
    }

    /// Get the current status of a connection
    pub async fn get_status(&self, uuid: &str) -> ConnectionStatus {
        let pools = self.pools.read().await;
        pools
            .get(uuid)
            .map(|e| e.status.clone())
            .unwrap_or(ConnectionStatus::Disconnected)
    }

    /// Get the last error for a connection
    pub async fn get_last_error(&self, uuid: &str) -> Option<String> {
        let pools = self.pools.read().await;
        pools.get(uuid).and_then(|e| e.last_error.clone())
    }

    /// Update last used time for a connection
    pub async fn touch(&self, uuid: &str) {
        let mut pools = self.pools.write().await;
        if let Some(entry) = pools.get_mut(uuid) {
            entry.last_used = Instant::now();
        }
    }

    /// Mark a connection as disconnected (e.g., after an error)
    pub async fn mark_disconnected(&self, uuid: &str, error: Option<String>) {
        let mut pools = self.pools.write().await;
        if let Some(entry) = pools.get_mut(uuid) {
            entry.status = ConnectionStatus::Disconnected;
            entry.last_error = error;
        }
    }

    /// Perform a health check on a connection
    pub async fn health_check(&self, uuid: &str) -> Result<TestConnectionResult, String> {
        let driver = {
            let pools = self.pools.read().await;
            pools.get(uuid).map(|e| e.driver.clone())
        };

        match driver {
            Some(driver) => {
                let result = driver.test_connection().await?;

                // Update status based on result
                {
                    let mut pools = self.pools.write().await;
                    if let Some(entry) = pools.get_mut(uuid) {
                        entry.status = if result.success {
                            ConnectionStatus::Connected
                        } else {
                            ConnectionStatus::Disconnected
                        };
                        entry.last_error = if result.success {
                            None
                        } else {
                            Some(result.message.clone())
                        };
                    }
                }

                Ok(result)
            }
            None => Ok(TestConnectionResult {
                success: false,
                message: "Connection not found".to_string(),
            }),
        }
    }

    /// Get a cached driver if it exists (without creating new connection)
    pub async fn get_cached(&self, uuid: &str) -> Option<Arc<Box<dyn DatabaseDriver>>> {
        let pools = self.pools.read().await;
        pools.get(uuid).map(|e| e.driver.clone())
    }

    /// Get config for a cached connection
    pub async fn get_config(&self, uuid: &str) -> Option<ConnectionConfig> {
        let pools = self.pools.read().await;
        pools.get(uuid).map(|e| e.config.clone())
    }
}
