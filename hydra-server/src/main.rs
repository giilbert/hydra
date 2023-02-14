mod container;
mod rpc;

use container::Container;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    log::info!("starting container");
    let mut container = Container::new().await?;

    let _response = container
        .rpc(protocol::ContainerRpcRequest {
            procedure: protocol::ContainerRpcProcedure::PtyCreate,
            parameters: serde_json::json!({}),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    let _response = container
        .rpc(protocol::ContainerRpcRequest {
            procedure: protocol::ContainerRpcProcedure::PtyInput,
            parameters: serde_json::json!({
                "id": 0,
                "input": "cd /\n",
            }),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let _response = container
        .rpc(protocol::ContainerRpcRequest {
            procedure: protocol::ContainerRpcProcedure::PtyInput,
            parameters: serde_json::json!({
                "id": 0,
                "input": "ls -a\n",
            }),
        })
        .await?;

    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    container.stop().await?;

    Ok(())
}
