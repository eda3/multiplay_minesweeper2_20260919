use std::hash::{BuildHasher, RandomState};
use tokio::net::TcpListener;

const DEFAULT_PORT: u16 = 8080;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = match std::env::var("PORT") {
        Ok(value) => value.parse()?,
        Err(_) => DEFAULT_PORT,
    };
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    eprintln!("listening on {}", listener.local_addr()?);
    // 盤面の乱数の種は、起動のたびに変える
    server::serve(listener, RandomState::new().hash_one(0_u8)).await
}
