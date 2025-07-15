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
use tokio::sync::RwLock;

use crate::{
    assets::{Asset, InMemoryAsset, get_all_assets},
    config::{Conf, PartialConf},
};

type AssetMap = Arc<RwLock<HashMap<String, InMemoryAsset>>>;

const SIMPLE_TIME_FORMAT: &[BorrowedFormatItem<'_>] =
    format_description!("[hour]:[minute]:[second]");

use axum::extract::{Request, State};
async fn serve_from_memory(State(assets): State<AssetMap>, req: Request) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');
    match assets.read().await.get(path) {
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

use once_cell::sync::Lazy;
use tokio::sync::broadcast;
static RELOAD_TX: Lazy<broadcast::Sender<String>> = Lazy::new(|| {
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

pub async fn run(partial_config: PartialConf, address: String, open: bool) -> anyhow::Result<()> {
    let config = Arc::new(Conf::from_partial(partial_config).unwrap());
    let docs_dir = &config.docs_dir;
    let state: AssetMap = Arc::new(RwLock::new(HashMap::new()));

    use notify::EventKind::{Create, Modify, Remove};
    use time::OffsetDateTime;
    let mut debouncer = new_debouncer(Duration::from_millis(250), None, {
        let config = Arc::clone(&config);
        let state = Arc::clone(&state);
        let rt = tokio::runtime::Handle::current();
        move |result: DebounceEventResult| {
            if let Ok(events) = &result {
                if events
                    .iter()
                    .any(|e| matches!(e.event.kind, Create(_) | Modify(_) | Remove(_)))
                {
                    let now = OffsetDateTime::now_local()
                        .unwrap_or(OffsetDateTime::now_utc())
                        .time()
                        .format(SIMPLE_TIME_FORMAT)
                        .unwrap_or("?".to_string());

                    println!("[{now}] Detected change in docs directory, rebuilding...");

                    if let Err(e) = rt.block_on(rebuild_in_memory_assets(&config, &state)) {
                        eprintln!("Error rebuilding assets: {e}");
                    }

                    let _ = RELOAD_TX.send("reload".to_string());
                }
            }
        }
    })
    .context("Failed to set up file watcher!")?;

    debouncer
        .watch(docs_dir, RecursiveMode::Recursive)
        .with_context(|| format!("Failed to watch docs_dir: {docs_dir:?}"))?;

    rebuild_in_memory_assets(&config, &state)
        .await
        .context("Failed to perform intial build")?;

    let app = Router::new()
        .fallback(get(serve_from_memory))
        .with_state(state)
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
    let _ = RELOAD_TX.send("reload".to_string());

    axum::serve(listener, app)
        .await
        .context("Failed to start server")?;

    Ok(())
}
