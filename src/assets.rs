use anyhow::Context;
use once_cell::sync::Lazy;
use pulldown_cmark::{Parser as MarkdownParser, html};
use std::fmt;
use std::path::PathBuf;
use tera::Tera;
use tokio::{task::JoinSet, try_join};
use walkdir::WalkDir;

use crate::config::Conf;

static TERA: Lazy<Tera> =
    Lazy::new(|| Tera::new("templates/*.html").expect("Failed to load templates"));

/// A built HTML file, ready to be dumped into the output directory or served
pub struct Page {
    pub content: String,
    pub url_path: String,
}

/// A static file (non-markdown) to be served or copied
pub struct StaticAsset {
    pub url_path: String,
    pub content: Vec<u8>,
    pub mime_type: mime_guess::Mime,
}

#[derive(Debug)]
pub enum InMemoryAsset {
    Page(Page),
    Static(StaticAsset),
}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field("url_path", &self.url_path)
            .field("content", &"<redacted>")
            .finish()
    }
}

impl fmt::Debug for StaticAsset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticAsset")
            .field("url_path", &self.url_path)
            .field("content", &format!("<{} bytes>", self.content.len()))
            .field("mime_type", &self.mime_type)
            .finish()
    }
}

pub async fn get_all_assets(config: &Conf) -> anyhow::Result<(Vec<Page>, Vec<StaticAsset>)> {
    let config_arc = std::sync::Arc::new(config.clone());
    let mut html_page_tasks = JoinSet::new();
    let mut static_asset_tasks = JoinSet::new();

    for entry in WalkDir::new(&config.docs_dir).follow_links(config.follow_links) {
        let source_path = entry
            .with_context(|| {
                format!(
                    "Error walking docs directory entry in {:?}",
                    config.docs_dir
                )
            })?
            .into_path();

        if !source_path.is_file() {
            continue;
        }

        let config_for_task = std::sync::Arc::clone(&config_arc);

        let relative_path = source_path
            .strip_prefix(&config.docs_dir)
            .with_context(|| {
                format!(
                    "Failed to strip prefix from path {:?} with docs_dir {:?}",
                    source_path, config.docs_dir
                )
            })?
            .to_owned();

        if source_path.extension() == Some(std::ffi::OsStr::new("md")) {
            html_page_tasks.spawn(async move {
                let markdown_content = tokio::fs::read_to_string(&source_path)
                    .await
                    .with_context(|| format!("Failed to read markdown file: {source_path:?}"))?;

                let mut rendered_content = String::new();
                html::push_html(
                    &mut rendered_content,
                    MarkdownParser::new(&markdown_content),
                );

                let url_path = if relative_path == PathBuf::from("index.md") {
                    "".to_string()
                } else {
                    relative_path
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                };

                let mut context = tera::Context::new();
                context.insert("config", &*config_for_task);
                context.insert("current_path", &url_path);
                context.insert("content", &rendered_content);

                let content = TERA.render("base.html", &context).with_context(|| {
                    format!("Failed to render Tera template for file: {source_path:?}")
                })?;

                Ok(Page {
                    content,
                    url_path: if url_path.is_empty() {
                        "index.html".to_string()
                    } else {
                        format!("{url_path}/index.html")
                    },
                })
            });
        } else {
            static_asset_tasks.spawn(async move {
                let content = tokio::fs::read(&source_path)
                    .await
                    .with_context(|| format!("Failed to read static file: {source_path:?}"))?;

                let mime_type = mime_guess::from_path(&source_path).first_or_octet_stream();

                Ok(StaticAsset {
                    url_path: relative_path.to_string_lossy().to_string(),
                    content,
                    mime_type,
                })
            });
        }
    }

    Ok(try_join!(
        async {
            html_page_tasks
                .join_all()
                .await
                .into_iter()
                .collect::<Result<_, anyhow::Error>>()
                .context("Failed to process one or more HTML pages")
        },
        async {
            static_asset_tasks
                .join_all()
                .await
                .into_iter()
                .collect::<Result<_, anyhow::Error>>()
                .context("Failed to process one or more static assets")
        },
    )?)
}
