use std::{env, error::Error, str::FromStr};

use rust_todos::{app, initialize_database};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://todos.db".to_owned());
    let options = SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true);
    let pool: SqlitePool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    initialize_database(&pool).await?;

    let address = env::var("APP_ADDRESS").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    let listener = TcpListener::bind(&address).await?;
    tracing::info!("listening on http://{address}");
    axum::serve(listener, app(pool)).await?;
    Ok(())
}
