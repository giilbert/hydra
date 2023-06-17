use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    extract::Host,
    handler::HandlerWithoutStateExt,
    http::{StatusCode, Uri},
    response::Redirect,
    routing::{get, post},
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use container::Container;
use execute::execute;
use pool::ContainerPool;
use run_request::RunRequest;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::execute::execute_websocket;

mod container;
mod execute;
mod pool;
mod rpc;
mod run_request;

type AppState = Arc<RwLock<AppStateInner>>;

#[derive(Debug)]
pub struct AppStateInner {
    pub run_requests: HashMap<Uuid, RunRequest>,
    pub container_pool: ContainerPool,
    pub api_key: String,
}

#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    pretty_env_logger::init();

    let state = AppState::new(RwLock::new(AppStateInner {
        run_requests: Default::default(),
        container_pool: ContainerPool::new(5).await,
        api_key: std::env::var("HYDRA_API_KEY").unwrap_or_else(|_| {
            log::warn!("No API key set. Using `hydra`.");
            "hydra".to_string()
        }),
    }));

    let router = Router::new()
        .route("/", get(|| async { "Hydra" }))
        .route("/execute", post(execute).get(execute_websocket))
        .with_state(state.clone())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        );

    let config = create_rustls_config().await;

    // run if it is not a production environment
    if std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()) != "production" {
        app_without_https_redirect(router.clone(), state.clone()).await;
        return Ok(());
    }

    let ports = Ports {
        http: 80,
        https: 443,
    };
    tokio::spawn(redirect_http_to_https(ports));

    let addr = SocketAddr::from(([0, 0, 0, 0], ports.https));
    log::info!("HTTPS listening on {}", addr);
    axum_server::bind_rustls(addr, config)
        .serve(router.with_state(state).into_make_service())
        .await
        .unwrap();

    Ok(())
}

async fn create_rustls_config() -> RustlsConfig {
    use tokio::fs;
    // if there is a "certs" directory in the current directory then use those
    // certificates, otherwise use the default ones in "test-certs"

    let (cert_pem, key_pem) = if fs::try_exists("certs").await.unwrap_or(false) {
        (
            fs::read("certs/cert.pem").await.unwrap(),
            fs::read("certs/key.pem").await.unwrap(),
        )
    } else {
        (
            fs::read("test-certs/cert.pem").await.unwrap(),
            fs::read("test-certs/key.pem").await.unwrap(),
        )
    };

    RustlsConfig::from_pem(cert_pem, key_pem).await.unwrap()
}

async fn redirect_http_to_https(ports: Ports) {
    fn make_https(host: String, uri: Uri, ports: Ports) -> anyhow::Result<Uri> {
        let mut parts = uri.into_parts();

        parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().unwrap());
        }

        let https_host = host.replace(&ports.http.to_string(), &ports.https.to_string());
        parts.authority = Some(https_host.parse()?);

        Ok(Uri::from_parts(parts)?)
    }

    let redirect = move |Host(host): Host, uri: Uri| async move {
        match make_https(host, uri, ports) {
            Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
            Err(error) => {
                log::warn!("failed to convert URI to HTTPS: {}", error);
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], ports.http));
    log::info!("HTTP -> HTTPS redirect listening on {}", addr);
    axum::Server::bind(&addr.into())
        .serve(redirect.into_make_service())
        .await
        .unwrap();
}

async fn app_without_https_redirect(router: Router<AppState>, state: AppState) {
    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    log::info!("Plain HTTP listening on {}", addr);
    axum::Server::bind(&addr.into())
        .serve(router.with_state(state).into_make_service())
        .await
        .unwrap();
}
