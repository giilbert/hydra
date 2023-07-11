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

use crate::{
    config::Config,
    execute::{execute_headless, execute_websocket},
};

mod config;
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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn get() -> Self {
        match std::env::var("ENVIRONMENT")
            .unwrap_or("development".to_string())
            .as_str()
        {
            "production" => Environment::Production,
            "development" => Environment::Development,
            _ => panic!("invalid environment"),
        }
    }
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
    Config::global();

    let environment = Environment::get();
    log::info!("Hello! Environment: {:?}", environment);

    let state = AppState::new(RwLock::new(AppStateInner {
        run_requests: Default::default(),
        api_key: std::env::var("HYDRA_API_KEY").unwrap_or_else(|_| {
            log::warn!("No API key set. Using `hydra`.");
            "hydra".to_string()
        }),
        container_pool: ContainerPool::new(if environment == Environment::Development {
            2
        } else {
            // // TESTING VALUE
            // 2
            8
        })
        .await,
    }));

    let router = Router::new()
        .route("/", get(|| async { "Hydra" }))
        .route("/execute", post(execute).get(execute_websocket))
        .route("/execute-headless", post(execute_headless))
        .with_state(state.clone())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_headers(Any)
                .allow_methods(Any),
        );

    let state_clone = state.clone();
    tokio::spawn(async move {
        // listen to the stop signal
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
        log::info!("Shutting down..");
        // removing all containers
        if let Err(e) = state_clone.read().await.container_pool.shutdown().await {
            log::error!("Failed to shutdown container pool: {}", e);
        }
        log::info!("Done");
        std::process::exit(0);
    });

    // run if it is not a production environment
    if environment == Environment::Development || !Config::global().use_https {
        app_without_https_redirect(router.clone(), state.clone()).await;
        return Ok(());
    }

    let config = create_rustls_config().await;

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
        log::warn!(
            "Using default test SSL certificates, this is insecure and will definitely not work."
        );
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
    let addr = SocketAddr::from(([0, 0, 0, 0], 3100));
    log::info!("Plain HTTP listening on {}", addr);
    axum::Server::bind(&addr.into())
        .serve(router.with_state(state).into_make_service())
        .await
        .unwrap();
}
