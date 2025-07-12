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
    assets::{InMemoryAsset, get_all_assets},
    config::Conf,
};

type AssetMap = Arc<RwLock<HashMap<String, InMemoryAsset>>>;

const SIMPLE_TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second]");

async fn serve_from_memory(
    Path(path): Path<String>,
    assets: axum::extract::Extension<AssetMap>,
) -> impl IntoResponse {
    dbg!(&assets);
    dbg!(&path);

    // FIXME: this is horrible, i think
    let path = if path.is_empty() {
        "index.html".to_string()
    } else if !path.contains('.') {
        format!("{path}/index.html")
    } else {
        path
    };

    if let Some(asset) = assets.read().await.get(&path) {
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
    let (html_pages, static_assets) = get_all_assets(config).await?;

    let map: HashMap<_, _> = html_pages
        .into_iter()
        .map(|page| (page.url_path.clone(), InMemoryAsset::Page(page)))
        .chain(
            static_assets
                .into_iter()
                .map(|asset| (asset.url_path.clone(), InMemoryAsset::Static(asset))),
        )
        .collect();

    *store.write().await = map;
    Ok(())
}

pub async fn run(config: Conf, address: String, open: bool) -> anyhow::Result<()> {
    let config = Arc::new(config);
    let docs_dir = config.docs_dir.clone();

    let store: AssetMap = Arc::new(RwLock::new(HashMap::new()));
    rebuild_in_memory_assets(&config, &store).await?;

    let store_clone = Arc::clone(&store);
    let config_clone = Arc::clone(&config);

    let livereload_layer = LiveReloadLayer::new();
    let reloader = livereload_layer.reloader();
    reloader.reload();

    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let rt = tokio::runtime::Handle::current();

        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            None,
            move |result: DebounceEventResult| {
                let tx = tx.clone();
                rt.spawn(async move {
                    if match &result {
                        Ok(events) => events.iter().any(|debounced_event| {
                            matches!(
                                debounced_event.event.kind,
                                notify::EventKind::Create(_)
                                    | notify::EventKind::Modify(_)
                                    | notify::EventKind::Remove(_)
                            )
                        }),
                        _ => false,
                    } {
                        if let Err(e) = tx.send(result).await {
                            eprintln!("Error sending event result: {e:?}");
                        }
                    }
                });
            },
        )
        .context("Failed to create file watcher")?;

        debouncer
            .watch(&docs_dir, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch docs_dir: {docs_dir:?}"))?;

        while rx.recv().await.is_some() {
            let now = OffsetDateTime::now_local()
                .unwrap_or(OffsetDateTime::now_utc())
                .time()
                .format(SIMPLE_TIME_FORMAT)
                .context("Failed to format time for log output")?;
            println!("[{now}] Detected change in docs directory, rebuilding...");
            if let Err(e) = rebuild_in_memory_assets(&config_clone, &store_clone).await {
                eprintln!("Failed to rebuild in-memory assets: {e:?}");
                continue;
            }
            reloader.reload();
        }
        Ok::<(), anyhow::Error>(())
    });

    let app = Router::new()
        .route(
            "/",
            get({
                let assets = axum::extract::Extension(store.clone());
                || serve_from_memory(Path("".to_string()), assets)
            }),
        )
        .route("/{*path}", get(serve_from_memory))
        .layer(axum::extract::Extension(store))
        .layer(livereload_layer);

    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("Failed to bind to address: {address}"))?;

    println!("Serving at http://{}", &address);
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
