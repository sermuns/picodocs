use anyhow::Context;
use axum::{
    Router,
    body::{self, Body},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use confique::Config;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use time::format_description::BorrowedFormatItem;
use time::macros::format_description;

use crate::{
    assets::{Asset, InMemoryAsset, get_all_assets},
    config::{Conf, PartialConf},
};

const SIMPLE_TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second]");

use axum::extract::{Request, State};

type AssetMapLock = Arc<RwLock<HashMap<String, InMemoryAsset>>>;

async fn serve_from_memory(
    State(asset_map_lock): State<AssetMapLock>,
    req: Request,
) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    let map = asset_map_lock.read().unwrap();

    match map.get(path) {
        Some(asset) => match asset {
            InMemoryAsset::Page(p) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/html")
                .body(Body::from(p.rendered.clone()))
                .unwrap()
                .into_response(),
            InMemoryAsset::Static(s) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", s.mime_type.as_ref())
                .body(Body::from(s.content.clone()))
                .unwrap()
                .into_response(),
        },
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from(format!("{path} not found")))
            .unwrap()
            .into_response(),
    }
}
fn rebuild_in_memory_assets(
    config: &Conf,
    store: &RwLock<HashMap<String, InMemoryAsset>>,
) -> anyhow::Result<()> {
    let mut map = store
        .write()
        .map_err(|e| anyhow::anyhow!("Failed to acquire write lock: {}", e))?;

    *map = get_all_assets(config)?
        .into_iter()
        .map(|asset| match asset {
            Asset::Page(page) => (page.url_path.clone(), InMemoryAsset::Page(page)),
            Asset::Static(static_asset) => (
                static_asset.url_path.clone(),
                InMemoryAsset::Static(static_asset),
            ),
        })
        .collect::<HashMap<_, _>>();

    Ok(())
}

use once_cell::sync::Lazy;
use tokio::sync::broadcast;
static RELOAD_TX: Lazy<broadcast::Sender<()>> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(100);
    tx
});

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::{Stream, StreamExt};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(RELOAD_TX.subscribe()).map(|_| {
        Result::<Event, Infallible>::Ok(
            Event::default()
                .retry(Duration::from_millis(250))
                .data("reload"),
        )
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-ping"),
    )
}

use axum::middleware::Next;

static LIVERELOAD_SCRIPT_BYTES: &[u8] = br#"<script>
    new EventSource('/~~~picodocs-reload').onmessage = (e) => {
        if (e.data === 'reload') window.location.reload()
    };
</script>"#;

async fn append_livereload_script(request: Request, next: Next) -> Response {
    let response = next.run(request).await;

    if response.status() != StatusCode::OK {
        return response;
    }

    let (mut parts, body) = response.into_parts();

    match parts.headers.get(hyper::header::CONTENT_TYPE) {
        Some(content_type) if content_type.to_str().unwrap_or("").contains("text/html") => {}
        _ => {
            // dont mess with non-html
            return Response::from_parts(parts, body);
        }
    }

    let body_bytes = body::to_bytes(body, usize::MAX).await.unwrap();

    let mut modified_body_bytes =
        Vec::with_capacity(body_bytes.len() + LIVERELOAD_SCRIPT_BYTES.len());
    modified_body_bytes.extend_from_slice(&body_bytes);
    modified_body_bytes.extend_from_slice(LIVERELOAD_SCRIPT_BYTES);

    parts.headers.remove(hyper::header::CONTENT_LENGTH);

    Response::from_parts(parts, body::Body::from(modified_body_bytes))
}

use std::sync::RwLock;

pub fn run(partial_config: PartialConf, address: String, open: bool) -> anyhow::Result<()> {
    let config = Arc::new(Conf::from_partial(partial_config).unwrap());
    let docs_dir = config.docs_dir.clone();

    let asset_map = Arc::new(RwLock::new(HashMap::new()));

    let config_for_thread = Arc::clone(&config);
    let asset_map_for_thread = Arc::clone(&asset_map);

    std::thread::spawn(move || {
        use notify::EventKind::{Create, Modify, Remove};
        use time::OffsetDateTime;

        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            None,
            move |res: DebounceEventResult| {
                if let Ok(events) = &res {
                    if events
                        .iter()
                        .any(|e| matches!(e.event.kind, Create(_) | Modify(_) | Remove(_)))
                    {
                        let now = OffsetDateTime::now_local()
                            .unwrap_or_else(|_| OffsetDateTime::now_utc())
                            .time()
                            .format(SIMPLE_TIME_FORMAT)
                            .unwrap_or_else(|_| "?".to_string());

                        println!("[{now}] Change detected, rebuilding...");

                        if let Err(e) =
                            rebuild_in_memory_assets(&config_for_thread, &asset_map_for_thread)
                        {
                            eprintln!("Error rebuilding assets: {}", e);
                        } else if let Err(e) = RELOAD_TX.send(()) {
                            eprintln!("Error sending reload message: {}", e);
                        }
                    }
                }
            },
        )
        .expect("Failed to set up file watcher");

        debouncer
            .watch(&docs_dir, RecursiveMode::Recursive)
            .expect("Failed to watch docs_dir");

        loop {
            std::thread::park();
        }
    });

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        rebuild_in_memory_assets(&config, &asset_map)?;

        let app = Router::new()
            .fallback(get(serve_from_memory))
            .with_state(Arc::clone(&asset_map))
            .layer(axum::middleware::from_fn(append_livereload_script))
            .route("/~~~picodocs-reload", get(sse_handler));

        let listener = tokio::net::TcpListener::bind(&address)
            .await
            .with_context(|| format!("Failed to bind to address: {address}"))?;

        if open {
            open::that(format!("http://{}", &address))
                .with_context(|| format!("Failed to open browser at http://{}", &address))?;
        }

        println!("Serving at http://{}", &address);
        let _ = RELOAD_TX.send(());

        axum::serve(listener, app)
            .await
            .context("Failed to start server")
    })
}
