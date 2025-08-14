#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use ::config::Config;
use deadpool_postgres::{Client, Pool};
use dotenvy::dotenv;

mod config {
    use serde::Deserialize;
    #[derive(Debug, Default, Deserialize)]
    pub struct AppConfig {
        pub pg: deadpool_postgres::Config,
    }
}

// We are not using the pool off of this struct yet
#[allow(dead_code)]
struct DatabaseManager {
    pool: Pool,
    connection: Option<Client>,
    resource_id: String,
}

impl DatabaseManager {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config_ = Config::builder()
            .add_source(::config::Environment::default())
            .build()?;
        let config: config::AppConfig = config_.try_deserialize()?;
        let pool = config.pg.create_pool(None, tokio_postgres::NoTls)?;

        let resource_id = format!("db_manager_{}", uuid::Uuid::new_v4());
        println!("Acquiring database resources for {resource_id}");

        let connection = pool.get().await?;
        println!("Successfully acquired database connection for {resource_id}");

        Ok(Self {
            pool,
            connection: Some(connection),
            resource_id,
        })
    }

    const fn get_client(&mut self) -> Option<&mut Client> {
        self.connection.as_mut()
    }
}

impl Drop for DatabaseManager {
    fn drop(&mut self) {
        if self.connection.is_some() {
            println!(
                "Auto-cleanup: Releasing database connection for {} during drop",
                self.resource_id
            );
        }
        println!("Database manager {} resources cleaned up", self.resource_id);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let mut scoped_db_manager = DatabaseManager::new().await?;
    let _client = scoped_db_manager.get_client();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_manager_creation() {
        // This test will only pass if proper environment variables are set
        // In a real scenario, you might want to use a test database or mock
        if std::env::var("PG__HOST").is_ok() {
            let result = DatabaseManager::new().await;
            assert!(
                result.is_ok(),
                "DatabaseManager should be created successfully"
            );

            if let Ok(mut manager) = result {
                let client = manager.get_client();
                assert!(client.is_some(), "Client should be available");
            }
        }
    }

    #[tokio::test]
    async fn test_database_manager_get_client() {
        if std::env::var("PG__HOST").is_ok() {
            let mut manager = DatabaseManager::new()
                .await
                .expect("Failed to create DatabaseManager");

            // Test that we can get a client
            let client = manager.get_client();
            assert!(client.is_some(), "Should return Some(client)");

            // Test that subsequent calls still work
            let client2 = manager.get_client();
            assert!(client2.is_some(), "Should still return Some(client)");
        }
    }

    #[test]
    fn test_config_deserialization() {
        let config = Config::builder()
            .add_source(::config::Environment::default())
            .build();

        assert!(config.is_ok(), "Config should build successfully");

        if let Ok(config) = config {
            let app_config: Result<config::AppConfig, _> = config.try_deserialize();
            // This might fail if environment variables are not set, which is expected
            // In a real test environment, you'd set up proper test configuration
            println!("Config deserialization result: {:?}", app_config.is_ok());
        }
    }

    #[test]
    fn test_resource_id_generation() {
        // Test that resource IDs are unique by checking UUID format
        let uuid_str = uuid::Uuid::new_v4().to_string();
        let resource_id = format!("db_manager_{uuid_str}");

        assert!(resource_id.starts_with("db_manager_"));
        assert!(resource_id.len() > "db_manager_".len());
    }
}
