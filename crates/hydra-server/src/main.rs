mod app_state;
mod config;
mod container;
mod execute;
mod pool;
mod proxy_interface;
mod proxy_websockets;
mod rpc;
mod session;
mod shutdown;

use crate::{
    app_state::AppState,
    config::{Config, Environment},
    execute::{execute_headless, execute_websocket},
    proxy_interface::proxy_websocket,
};
use axum::{
    extract::Host,
    handler::HandlerWithoutStateExt,
    http::{StatusCode, Uri},
    middleware,
    response::Redirect,
    routing::{any, get, post},
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use container::Container;
use execute::execute;
use proxy_interface::proxy_http;
use shared::prelude::*;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().expect("failed to install color_eyre");
    pretty_env_logger::init();
    dotenv::dotenv().ok();

    Config::global();

    let environment = Environment::get();
    log::info!("Hello! Environment: {:?}", environment);

    let state = AppState::create_with_defaults(environment).await;

    tokio::spawn(shutdown::signal_handler(state.clone()));
    tokio::spawn(shutdown::run_check_test(state.clone()));

    let router = Router::new()
        .route("/", get(|| async { "Hydra" }))
        .route("/execute", post(execute).get(execute_websocket))
        .route("/execute-headless", post(execute_headless))
        .route("/proxy", any(proxy_http))
        .route("/proxy-websocket", get(proxy_websocket))
        .with_state(state.clone())
        .layer(middleware::from_fn(
            shutdown::update_last_activity_middleware,
        ));

    // run if it is not a production environment
    if environment == Environment::Development || !Config::global().use_https {
        // if it is a fly environment, we need to listen on the 6pn address too
        if std::env::var("FLY_PRIVATE_IP").is_ok() {
            tokio::join!(
                app_listen_on_fly_6pn(router.clone(), state.clone()),
                app_without_https_redirect(router.clone(), state.clone()),
            );
        } else {
            app_without_https_redirect(router.clone(), state.clone()).await;
        }

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
        .expect("unable to bind to port");

    Ok(())
}

async fn create_rustls_config() -> RustlsConfig {
    use tokio::fs;
    // if there is a "certs" directory in the current directory then use those
    // certificates, otherwise use the default ones in "test-certs"

    let (cert_pem, key_pem) = if fs::try_exists("certs").await.unwrap_or(false) {
        (
            fs::read("certs/cert.pem")
                .await
                .expect("error reading cert.pem"),
            fs::read("certs/key.pem")
                .await
                .expect("error reading key.pem"),
        )
    } else {
        log::warn!(
            "Using default test SSL certificates, this is insecure and will definitely not work."
        );
        (
            fs::read("test-certs/cert.pem")
                .await
                .expect("error reading cert.pem"),
            fs::read("test-certs/key.pem")
                .await
                .expect("error reading key.pem"),
        )
    };

    RustlsConfig::from_pem(cert_pem, key_pem)
        .await
        .expect("invalid cert/key provided")
}

async fn redirect_http_to_https(ports: Ports) {
    fn make_https(host: String, uri: Uri, ports: Ports) -> Result<Uri> {
        let mut parts = uri.into_parts();

        parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().expect("failed to parse path"));
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
        .expect("unable to bind to port");
}

async fn app_without_https_redirect(router: Router<AppState>, state: AppState) {
    let addr = SocketAddr::from(([0, 0, 0, 0], 3100));
    log::info!("Plain HTTP listening on {}", addr);
    axum::Server::bind(&addr.into())
        .serve(router.with_state(state).into_make_service())
        .await
        .expect("unable to bind to port");
}

async fn app_listen_on_fly_6pn(router: Router<AppState>, state: AppState) {
    // bind to fly-local-6pn:3100

    let addr = SocketAddr::from((
        dns_lookup::lookup_host("fly-local-6pn")
            .expect("failed to lookup fly-local-6pn")
            .into_iter()
            .next()
            .expect("no addresses found for fly-local-6pn"),
        3100,
    ));

    log::info!("HTTP (6PN) listening on {}", addr);
    axum::Server::bind(&addr.into())
        .serve(router.with_state(state).into_make_service())
        .await
        .expect("unable to bind to port");
}
