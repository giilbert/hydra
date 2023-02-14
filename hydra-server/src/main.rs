mod container;
mod rpc;

use container::Container;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    log::info!("starting container");
    let mut container = Container::new().await?;

    let now = std::time::Instant::now();

    let response = container
        .rpc(protocol::ContainerRpcRequest {
            procedure: protocol::ContainerRpcProcedure::PtyCreate,
            parameters: serde_json::json!({}),
        })
        .await?;

    log::info!("rpc response: {:?}", response);

    let elapsed = now.elapsed();
    log::info!("elapsed: {:?}", elapsed);

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let response = container
        .rpc(protocol::ContainerRpcRequest {
            procedure: protocol::ContainerRpcProcedure::PtyInput,
            parameters: serde_json::json!({
                "id": 0,
                "input": "exit\n",
            }),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    container.stop().await?;

    Ok(())
}
