use anyhow::Context;
use once_cell::sync::Lazy;
use pulldown_cmark::{Parser as MarkdownParser, html};
use std::path::{Path, PathBuf};
use tera::Tera;
use tokio::task::JoinSet;
use walkdir::WalkDir;

use crate::config::Conf;

static TERA: Lazy<Tera> =
    Lazy::new(|| Tera::new("templates/*.html").expect("Failed to load templates"));

/// A built HTML file, ready to be dumped into the output directory or served
#[derive(Debug)]
pub struct Page {
    pub content: String,
    pub url_path: String,
}

/// A static file (non-markdown) to be served or copied
#[derive(Debug)]
pub struct StaticAsset {
    pub source_path: PathBuf,
    pub url_path: String,
    pub content: Vec<u8>,
    pub mime_type: mime_guess::Mime,
}

#[derive(Debug)]
pub enum InMemoryAsset {
    Page(Page),
    Static(StaticAsset),
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

        let url_path = relative_path.to_string_lossy().into_owned();

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

                let mut context = tera::Context::new();
                context.insert("config", &*config_for_task);
                context.insert("content", &rendered_content);

                let content = TERA.render("base.html", &context).with_context(|| {
                    format!("Failed to render Tera template for file: {source_path:?}")
                })?;

                let file_stem = relative_path.file_stem().ok_or_else(|| {
                    anyhow::anyhow!("Could not get file stem for {:?}", relative_path)
                })?;

                let url_path = if relative_path == PathBuf::from("index.md") {
                    "".to_string()
                } else {
                    let parent_dir = relative_path.parent().unwrap_or_else(|| Path::new(""));

                    let mut url_path_buf = PathBuf::new();
                    url_path_buf.push(parent_dir);
                    url_path_buf.push(file_stem);
                    url_path_buf.to_string_lossy().into_owned()
                };

                Ok(Page { content, url_path })
            });
        } else {
            static_asset_tasks.spawn(async move {
                let content = tokio::fs::read(&source_path)
                    .await
                    .with_context(|| format!("Failed to read static file: {source_path:?}"))?;

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
        .collect::<Result<_, anyhow::Error>>()
        .context("Failed to process one or more HTML pages")?;

    let static_assets: Vec<StaticAsset> = static_asset_tasks
        .join_all()
        .await
        .into_iter()
        .collect::<Result<_, anyhow::Error>>()
        .context("Failed to process one or more static assets")?;

    Ok((html_pages, static_assets))
}
