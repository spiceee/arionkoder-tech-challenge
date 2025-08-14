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
