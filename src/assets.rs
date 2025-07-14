use anyhow::Context;
use once_cell::sync::Lazy;
use pulldown_cmark::{Options, Parser, html};
use serde::Serialize;
use std::fmt;
use std::path::PathBuf;
use tera::Tera;
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

use anyhow::Result;
use futures::future::join_all;
use std::collections::BTreeMap;
use std::path::Path;
use std::{ffi::OsStr, sync::Arc};

#[derive(Debug, Serialize)]
pub struct SitemapNode {
    pub title: String,
    pub path: Option<String>,
    pub children: Vec<SitemapNode>,
}

impl SitemapNode {
    pub fn new(pages: &[(PathBuf, PathBuf)]) -> Self {
        fn build(base: &Path, paths: &[PathBuf]) -> Vec<SitemapNode> {
            let mut map: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();

            for path in paths {
                if let Some((first, rest)) = path.iter().collect::<Vec<_>>().split_first() {
                    let entry = map.entry(first.to_string_lossy().into_owned()).or_default();
                    entry.push(PathBuf::from_iter(rest.iter()));
                }
            }

            map.into_iter()
                .filter_map(|(segment, children)| {
                    let full_path = base.join(&segment);
                    let all_empty = children.iter().all(|p| p.as_os_str().is_empty());

                    if all_empty {
                        if full_path.file_name() == Some(OsStr::new("index.md"))
                            && full_path.parent().is_some()
                        {
                            return None;
                        }
                        let path = if full_path == Path::new("index.md") {
                            "".to_string()
                        } else {
                            full_path.with_extension("").to_string_lossy().to_string()
                        };

                        let file_stem =
                            full_path.file_stem().unwrap().to_string_lossy().to_string();

                        Some(SitemapNode {
                            title: file_stem,
                            path: Some(path),
                            children: Vec::new(),
                        })
                    } else {
                        let path = if full_path == Path::new("index.md") {
                            "".to_string()
                        } else {
                            full_path.with_extension("").to_string_lossy().to_string()
                        };
                        Some(SitemapNode {
                            title: segment,
                            path: Some(path),
                            children: build(&full_path, &children),
                        })
                    }
                })
                .collect()
        }

        SitemapNode {
            title: "".to_string(),
            path: None,
            children: build(
                Path::new(""),
                &pages.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>(),
            ),
        }
    }
}

pub enum Asset {
    Page(Page),
    Static(StaticAsset),
}

static MARKDOWN_OPTIONS: Lazy<Options> = Lazy::new(|| {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    options
});

fn render_single_markdown_page(md: &str) -> String {
    use pulldown_cmark::{CowStr, Event, HeadingLevel, Tag};

    let mut previous_heading_level: Option<HeadingLevel> = None;
    let parser = Parser::new_ext(md, *MARKDOWN_OPTIONS).filter_map(|event| match event {
        Event::Start(Tag::Heading { level, .. }) => {
            previous_heading_level = Some(level);
            None
        }
        Event::Text(text) => {
            let Some(heading_level) = previous_heading_level.take() else {
                return Some(Event::Text(text));
            };

            let anchor: String = text
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect();

            let heading_start_and_text = Event::InlineHtml(CowStr::from(format!(
                "<h{} id=\"{}\">{}",
                heading_level as u8, anchor, text,
            )));

            previous_heading_level = None;
            Some(heading_start_and_text)
        }
        _ => Some(event),
    });

    // reasonable guess for HTML size?
    let mut html = String::with_capacity((md.len() * 3) / 2);
    html::push_html(&mut html, parser);
    html
}

/// Read all files from `conf.docs_dir`, return generated assets.
pub async fn get_all_assets(conf: &Conf) -> Result<Vec<Asset>> {
    let files: Vec<(PathBuf, PathBuf)> = WalkDir::new(&conf.docs_dir)
        .follow_links(conf.follow_links)
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().is_dir() {
                return None;
            }
            // Some(entry)

            let source_path = entry.path();
            let relative_path = source_path.strip_prefix(&conf.docs_dir).unwrap();

            Some((source_path.into(), relative_path.into()))
        })
        .collect();

    let markdown_extension = OsStr::new("md");
    let (pages, assets): (Vec<_>, Vec<_>) = files
        .into_iter()
        .partition(|(src, _)| src.extension() == Some(markdown_extension));

    let sitemap = Arc::new(SitemapNode::new(&pages));

    let page_render_tasks = pages.into_iter().map(|(src, rel)| {
        let conf = conf.clone();
        let sitemap = Arc::clone(&sitemap);
        tokio::spawn(async move {
            let md = tokio::fs::read_to_string(&src)
                .await
                .with_context(|| format!("Failed to read markdown file {src:?}"))?;

            let current_path = {
                let mut current_path = rel.clone();
                if rel.file_name() == Some(OsStr::new("index.md")) {
                    current_path.pop();
                } else {
                    current_path.set_extension(""); // Remove the extension
                }
                current_path.to_str().unwrap().to_string()
            };

            // FIXME: does this move `md` into function?
            let html = render_single_markdown_page(&md);

            let mut ctx = tera::Context::new();
            ctx.insert("config", &conf);
            ctx.insert("sitemap", &*sitemap);
            ctx.insert("current_path", &current_path);
            ctx.insert("content", &html);

            let rendered = TERA
                .render("base.html", &ctx)
                .with_context(|| format!("Render template for {src:?}"))?;

            Ok(Page {
                content: rendered,
                url_path: current_path,
            })
        })
    });

    let asset_render_tasks = assets.into_iter().map(|(src, rel)| {
        tokio::spawn(async move {
            Ok(StaticAsset {
                content: tokio::fs::read(&src)
                    .await
                    .with_context(|| format!("Read static file {src:?}"))?,
                url_path: rel.to_string_lossy().into_owned(),
                mime_type: mime_guess::from_path(&src).first_or_octet_stream(),
            })
        })
    });

    let page_assets = join_all(page_render_tasks)
        .await
        .into_iter()
        .map(|res| res.map_err(|e| anyhow::anyhow!(e)).and_then(|r| r))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .map(Asset::Page);

    let static_assets = join_all(asset_render_tasks)
        .await
        .into_iter()
        .map(|res| res.map_err(|e| anyhow::anyhow!(e)).and_then(|r| r))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .map(Asset::Static);

    Ok(page_assets.chain(static_assets).collect())
}
