use anyhow::Context;
use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::get,
};
use clap::{Parser, Subcommand};
use confique::{Config, File, FileFormat, Partial};
use notify::{
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{AccessKind, ModifyKind},
};
use once_cell::sync::Lazy;
use pulldown_cmark::{Parser as MarkdownParser, html};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tera::Tera;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[clap(version, author, about)]
struct Args {
    /// Config file
    #[arg(short, long = "config", default_value = "picodocs.toml")]
    config_path: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Builds the site
    Build {},

    /// Continually watches the docs directory for changes and rebuilds the site
    Serve {
        #[arg(short, long, default_value = "localhost:1809")]
        address: String,
    },

    /// Dump the default configuration to a file
    Defaults {
        #[arg(short, long, default_value = "picodocs.toml")]
        output_path: PathBuf,

        /// Overwrite the output file if it already exists
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Config, Clone, Debug, Serialize)]
struct Conf {
    title: Option<String>,

    /// Used as favicon, among other places
    icon: Option<PathBuf>,

    description: Option<String>,

    /// Sitemap will only generate if this is a full/absolute URL e.g. https://www.example.com/
    #[config(default = "/")]
    base_url: String,

    /// Root directory of markdown documentation
    #[config(default = "docs")]
    docs_dir: PathBuf,

    /// Where to place rendered site files
    #[config(default = "public")]
    output_dir: PathBuf,

    /// Follow symbolic links when traversing the docs directory
    #[config(default = false)]
    follow_links: bool,
}

type PartialConf = <Conf as Config>::Partial;

static TERA: Lazy<Tera> =
    Lazy::new(|| Tera::new("templates/*.html").expect("Failed to load templates"));

/// A built HTML file, ready to be dumped into the output directory or served
#[derive(Debug)]
struct Page {
    source_path: PathBuf,
    content: String,
    url_path: String,
}

/// A static file (non-markdown) to be served or copied
#[derive(Debug)]
struct StaticAsset {
    source_path: PathBuf,
    url_path: String,
    content: Vec<u8>,
    mime_type: mime_guess::Mime,
}

#[derive(Debug)]
enum InMemoryAsset {
    Page(Page),
    Static(StaticAsset),
}

type AssetMap = Arc<RwLock<HashMap<String, InMemoryAsset>>>;

async fn get_all_assets(config: &Conf) -> anyhow::Result<(Vec<Page>, Vec<StaticAsset>)> {
    let config_arc = Arc::new(config.clone());
    let mut html_page_tasks = JoinSet::new();
    let mut static_asset_tasks = JoinSet::new();

    for entry in WalkDir::new(&config.docs_dir).follow_links(config.follow_links) {
        let source_path = entry?.into_path();

        if !source_path.is_file() {
            continue;
        }

        let config_for_task = Arc::clone(&config_arc);

        // Clone these early so `path` is no longer borrowed when we move it later.
        let relative_path = source_path.strip_prefix(&config.docs_dir)?.to_owned();

        let url_path = relative_path.to_string_lossy().into_owned();

        if source_path.extension() == Some(std::ffi::OsStr::new("md")) {
            html_page_tasks.spawn(async move {
                let markdown_content = tokio::fs::read_to_string(&source_path)
                    .await
                    .with_context(|| format!("Failed to read markdown file: {:?}", source_path))?;

                let mut rendered_content = String::new();
                html::push_html(
                    &mut rendered_content,
                    MarkdownParser::new(&markdown_content),
                );

                let mut context = tera::Context::new();
                context.insert("config", &*config_for_task);
                context.insert("content", &rendered_content);

                let content = TERA.render("base.html", &context).with_context(|| {
                    format!("Failed to render Tera template for file: {:?}", source_path)
                })?;

                let file_stem = relative_path.file_stem().ok_or_else(|| {
                    anyhow::anyhow!("Could not get file stem for {:?}", relative_path)
                })?;

                let url_path = if relative_path == PathBuf::from("index.md") {
                    "".to_string()
                } else {
                    let parent_dir = relative_path.parent().unwrap_or_else(|| "".as_ref());

                    let mut url_path_buf = PathBuf::new();
                    url_path_buf.push(parent_dir);
                    url_path_buf.push(file_stem);
                    url_path_buf.to_string_lossy().into_owned()
                };

                Ok(Page {
                    source_path,
                    content,
                    url_path,
                })
            });
        } else {
            static_asset_tasks.spawn(async move {
                let content = tokio::fs::read(&source_path)
                    .await
                    .with_context(|| format!("Failed to read static file: {:?}", source_path))?;

                let mime_type = mime_guess::from_path(&source_path).first_or_octet_stream();

                Ok(StaticAsset {
                    source_path,
                    url_path,
                    content,
                    mime_type,
                })
            });
        }
    }

    let html_pages: Vec<Page> = html_page_tasks
        .join_all()
        .await
        .into_iter()
        .collect::<Result<_, anyhow::Error>>()?;

    let static_assets: Vec<StaticAsset> = static_asset_tasks
        .join_all()
        .await
        .into_iter()
        .collect::<Result<_, anyhow::Error>>()?;

    Ok((html_pages, static_assets))
}

async fn serve_from_memory(
    Path(path): Path<String>,
    assets: axum::extract::Extension<AssetMap>,
) -> impl IntoResponse {
    dbg!(&path);

    let map = assets.read().await;
    if let Some(asset) = map.get(&path) {
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
        .body(Body::from(format!("Asset not found for path: {}", path)))
        .unwrap()
        .into_response()
}

async fn rebuild_in_memory_assets(config: &Conf, store: &AssetMap) -> anyhow::Result<()> {
    println!("Rebuilding in-memory assets...");
    let (html_pages, static_assets) = get_all_assets(config).await?;

    let mut map = HashMap::new();

    for page in html_pages {
        map.insert(page.url_path.clone(), InMemoryAsset::Page(page));
    }

    for asset in static_assets {
        map.insert(asset.url_path.clone(), InMemoryAsset::Static(asset));
    }

    *store.write().await = map;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = if (&args.config_path).exists() {
        let partial_conf: PartialConf = File::with_format(&args.config_path, FileFormat::Toml)
            .required()
            .load()?;
        Conf::from_partial(partial_conf.with_fallback(PartialConf::default_values()))?
    } else {
        Conf::from_partial(PartialConf::default_values())?
    };

    match args.command {
        Command::Build {} => {
            let before_build = Instant::now();
            println!("Building site with configuration: {:?}", config);

            if config.output_dir.exists() {
                tokio::fs::remove_dir_all(&config.output_dir)
                    .await
                    .with_context(|| {
                        format!("Failed to remove output directory: {:?}", config.output_dir)
                    })?;
            }

            let (html_pages, static_assets) = get_all_assets(&config).await?;

            for page in html_pages {
                let output_path = config.output_dir.join(&page.url_path).join("index.html");
                if let Some(parent) = output_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                }
                tokio::fs::write(&output_path, page.content)
                    .await
                    .with_context(|| format!("Failed to write HTML file: {:?}", output_path))?;
            }
            for asset in static_assets {
                let output_path = config
                    .output_dir
                    .join(&asset.source_path.strip_prefix(&config.docs_dir)?);
                if let Some(parent) = output_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("Failed to create directory: {:?}", parent))?;
                }
                tokio::fs::write(&output_path, &asset.content)
                    .await
                    .with_context(|| format!("Failed to write static asset: {:?}", output_path))?;
            }
            println!("Site built in {:?}", before_build.elapsed());
        }

        Command::Serve { address } => {
            let config = Arc::new(config);
            let docs_dir = config.docs_dir.clone();

            let store: AssetMap = Arc::new(RwLock::new(HashMap::new()));
            rebuild_in_memory_assets(&config, &store).await?;

            let store_clone = Arc::clone(&store);
            let config_clone = Arc::clone(&config);

            tokio::spawn(async move {
                let (tx, mut rx) = tokio::sync::mpsc::channel(1);

                let mut watcher = RecommendedWatcher::new(
                    move |res: Result<notify::Event, notify::Error>| {
                        if let Ok(event) = res {
                            match &event.kind {
                                EventKind::Create(_)
                                | EventKind::Remove(_)
                                | EventKind::Modify(_) => {
                                    let _ = tx.try_send(());
                                }
                                _ => {}
                            }
                        }
                    },
                    notify::Config::default()
                        .with_poll_interval(Duration::from_secs(1))
                        .with_compare_contents(true)
                        .with_follow_symlinks(config.follow_links),
                )
                .expect("Failed to create file watcher");

                watcher
                    .watch(&docs_dir, RecursiveMode::Recursive)
                    .expect("Failed to watch docs_dir");

                while rx.recv().await.is_some() {
                    if let Err(e) = rebuild_in_memory_assets(&config_clone, &store_clone).await {
                        eprintln!("Failed to rebuild in-memory assets: {:?}", e);
                    }
                }
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
                .layer(axum::extract::Extension(store));

            let listener = tokio::net::TcpListener::bind(&address).await.unwrap();

            println!("Serving at http://{}", &address);
            axum::serve(listener, app)
                .await
                .context("Failed to start server")?;
        }

        Command::Defaults { output_path, force } => {
            if output_path.exists() && (force == false) {
                return Err(anyhow::anyhow!(
                    "{:?} already exists. Aborting.",
                    &output_path
                ));
            }

            let default_conf = Conf::from_partial(PartialConf::default_values())?;

            tokio::fs::write(&output_path, toml::to_string(&default_conf)?)
                .await
                .with_context(|| {
                    format!(
                        "Failed to write default configuration to {:?}",
                        &output_path
                    )
                })?;
            println!("Default configuration written to {:?}", &output_path);
        }
    }

    Ok(())
}
