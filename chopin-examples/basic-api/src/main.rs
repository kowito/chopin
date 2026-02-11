use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let app = chopin_core::App::new().await?;
    app.run().await?;

    Ok(())
}
