mod config;
mod store;

pub use config::{check_connection, initialize_database, DatabaseConfig};
pub use store::store_results;
