mod container;
mod rpc;

use container::Container;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut container = Container::new().await?;

    let now = std::time::Instant::now();

    let response = container
        .rpc(protocol::ContainerRpcRequest {
            procedure: protocol::ContainerRpcProcedure::Test,
            parameters: serde_json::json!({
                "hello": 10,
            }),
        })
        .await?;

    log::info!("rpc response: {:?}", response);

    let elapsed = now.elapsed();
    log::info!("elapsed: {:?}", elapsed);

    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    container.stop().await?;

    Ok(())
}
