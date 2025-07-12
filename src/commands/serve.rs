use anyhow::Context;
use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::get,
};
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use time::macros::format_description;
use time::{OffsetDateTime, format_description::BorrowedFormatItem};
use tokio::sync::RwLock;
use tower_livereload::LiveReloadLayer;

use crate::{
    assets::{Asset, InMemoryAsset, get_all_assets},
    config::Conf,
};

type AssetMap = Arc<RwLock<HashMap<String, InMemoryAsset>>>;

const SIMPLE_TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second]");

use axum::extract::{Request, State};
async fn serve_from_memory(State(assets): State<AssetMap>, req: Request) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');
    if let Some(asset) = assets.read().await.get(path) {
        return match asset {
            InMemoryAsset::Page(p) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .body(Body::from(p.content.clone()))
                .unwrap()
                .into_response(),
            InMemoryAsset::Static(s) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", s.mime_type.as_ref())
                .body(Body::from(s.content.clone()))
                .unwrap()
                .into_response(),
        };
    }

    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from(format!("{path} not found")))
        .unwrap()
        .into_response()
}

async fn rebuild_in_memory_assets(config: &Conf, store: &AssetMap) -> anyhow::Result<()> {
    let assets = get_all_assets(config).await?;

    let updated_asset_map = assets
        .into_iter()
        .map(|asset| match asset {
            Asset::Page(page) => (page.url_path.clone(), InMemoryAsset::Page(page)),
            Asset::Static(static_asset) => (
                static_asset.url_path.clone(),
                InMemoryAsset::Static(static_asset),
            ),
        })
        .collect::<HashMap<_, _>>();

    *store.write().await = updated_asset_map;
    Ok(())
}

pub async fn run(config: Conf, address: String, open: bool) -> anyhow::Result<()> {
    let config = Arc::new(config);
    let docs_dir = config.docs_dir.clone();

    let livereload_layer = LiveReloadLayer::new();
    let reloader = livereload_layer.reloader();

    use notify::EventKind::{Create, Modify, Remove};
    let mut debouncer = new_debouncer(Duration::from_millis(250), None, {
        move |result: DebounceEventResult| {
            if let Ok(events) = &result {
                if events
                    .iter()
                    .any(|e| matches!(e.event.kind, Create(_) | Modify(_) | Remove(_)))
                {
                    println!("Event!");
                }
            }
        }
    })
    .context("Failed to set up file watcher!")?;

    debouncer
        .watch(&docs_dir, RecursiveMode::Recursive)
        .with_context(|| format!("Failed to watch docs_dir: {docs_dir:?}"))?;

    let state: AssetMap = Arc::new(RwLock::new(HashMap::new()));
    rebuild_in_memory_assets(&config, &state).await?;

    let app = Router::new()
        .fallback(get(serve_from_memory))
        .with_state(state)
        .layer(livereload_layer);

    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("Failed to bind to address: {address}"))?;

    println!("Serving at http://{}", &address);
    reloader.reload();
    if open {
        if let Err(e) = open::that(format!("http://{}", &address)) {
            eprintln!("Failed to open browser: {e}");
        }
    }
    axum::serve(listener, app)
        .await
        .context("Failed to start server")?;

    Ok(())
}
