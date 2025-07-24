use anyhow::Context;
use once_cell::sync::Lazy;
use pulldown_cmark::Options;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use tera::Tera;
use walkdir::WalkDir;

use crate::config::Conf;

static TERA: Lazy<Tera> =
    Lazy::new(|| Tera::new("templates/*.html").expect("Failed to load templates"));

/// A built HTML file, ready to be dumped into the output directory or served
#[derive(Clone)]
pub struct Page {
    pub rendered: String,
    pub url_path: String,
    pub front_matter: Option<FrontMatter>,
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
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct SitemapNode {
    pub title: String,
    pub path: Option<String>,
    pub children: Vec<SitemapNode>,
}

impl SitemapNode {
    pub fn new(pages: &[Page]) -> Self {
        fn build(path_prefix: &Path, pages: &[Page]) -> Vec<SitemapNode> {
            let mut groups: Vec<String> = Vec::new();
            let mut remaining_pages_for_group: BTreeMap<String, Vec<Page>> = BTreeMap::new();

            for page in pages {
                let relative_path_str = page
                    .url_path
                    .strip_prefix(&path_prefix.to_string_lossy().to_string())
                    .unwrap_or(&page.url_path);

                let parts: Vec<&str> = relative_path_str.split('/').collect();

                if let Some(first_segment) = parts.first() {
                    let segment = first_segment.to_string();
                    groups.push(segment.clone());

                    let mut remaining_path_buf = PathBuf::from("");
                    for (i, part) in parts.iter().enumerate() {
                        if i > 0 {
                            remaining_path_buf.push(part);
                        }
                    }

                    let mut page_for_child_call = page.clone();
                    page_for_child_call.url_path =
                        remaining_path_buf.to_string_lossy().into_owned();

                    remaining_pages_for_group
                        .entry(segment)
                        .or_default()
                        .push(page_for_child_call);
                } else {
                    let segment = PathBuf::from(&page.url_path)
                        .file_stem()
                        .unwrap_or(OsStr::new(""))
                        .to_string_lossy()
                        .into_owned();
                    groups.push(segment.clone());
                    remaining_pages_for_group
                        .entry(segment)
                        .or_default()
                        .push(page.clone()); // Still add for later processing
                }
            }

            groups
                .into_iter()
                .filter_map(|segment| {
                    let current_segment_path = PathBuf::from(&segment);
                    let full_node_path = path_prefix.join(&current_segment_path);
                    let children_for_recursion = remaining_pages_for_group
                        .get(&segment)
                        .cloned()
                        .unwrap_or_default();

                    let is_leaf = children_for_recursion
                        .iter()
                        .all(|p| p.url_path.is_empty() || p.url_path == "index.md");

                    if segment == "index.md"
                        && path_prefix != Path::new("")
                        && children_for_recursion.is_empty()
                    {
                        return None;
                    }

                    let path_for_sitemap_entry = if segment == "index.md" {
                        full_node_path
                            .parent()
                            .map_or_else(|| "".to_string(), |p| p.to_string_lossy().into_owned())
                    } else {
                        full_node_path.to_string_lossy().into_owned()
                    };

                    Some(SitemapNode {
                        title: current_segment_path
                            .file_stem()
                            .unwrap_or_else(|| OsStr::new(""))
                            .to_string_lossy()
                            .to_string(),
                        path: Some(path_for_sitemap_entry),
                        children: if is_leaf {
                            Vec::new()
                        } else {
                            build(&full_node_path, &children_for_recursion)
                        },
                    })
                })
                .collect()
        }

        SitemapNode {
            title: "".to_string(),
            path: None,
            children: build(Path::new(""), pages),
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

pub fn extract_front_matter(md: &str) -> Result<(Option<FrontMatter>, &str)> {
    const DELIMITER: &str = "---";

    if !md.starts_with(DELIMITER) {
        return Ok((None, md));
    }

    let content = &md[DELIMITER.len()..];
    let end = content
        .find(DELIMITER)
        .context("failed to find end of front matter")?;

    let fm: FrontMatter =
        serde_yaml::from_str(&content[..end]).context("failed to parse front matter")?;

    let rest = &content[end + DELIMITER.len()..];

    Ok((Some(fm), rest))
}

fn render_single_markdown_page(md: &str) -> (String, Option<FrontMatter>) {
    use pulldown_cmark::{CowStr, Event, HeadingLevel, Parser, Tag, html};

    let (front_matter, rest) = extract_front_matter(md).unwrap_or((None, md));

    let mut previous_heading_level: Option<HeadingLevel> = None;
    let parser = Parser::new_ext(rest, *MARKDOWN_OPTIONS).filter_map(|event| match event {
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

    (html, front_matter)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FrontMatter {
    title: Option<String>,
    description: Option<String>,
    keywords: Option<Vec<String>>,
}

/// Read all files from `conf.docs_dir`, return generated assets.
pub fn get_all_assets(config: &Conf) -> Result<Vec<Asset>> {
    let file_relative_paths: Vec<PathBuf> = WalkDir::new(&config.docs_dir)
        .follow_links(config.follow_links)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(&config.docs_dir)
                .ok()
                .map(PathBuf::from)
        })
        .collect();

    let mut all_assets = Vec::with_capacity(file_relative_paths.len());

    let (page_relative_paths, static_relative_paths): (Vec<_>, Vec<_>) = file_relative_paths
        .into_iter()
        .partition(|rel| rel.extension() == Some(OsStr::new("md")));

    let pages: Vec<Page> = page_relative_paths
        .into_iter()
        .map(|rel| {
            let md = std::fs::read_to_string(config.docs_dir.join(&rel))
                .with_context(|| format!("Failed to read markdown file {rel:?}"))?;

            let (html, front_matter) = render_single_markdown_page(&md);

            let current_path = {
                let mut p = rel.clone();
                if rel.file_name() == Some(OsStr::new("index.md")) {
                    p.pop();
                } else {
                    p.set_extension("");
                }
                p.to_str().unwrap().to_string()
            };

            Ok(Page {
                rendered: html,
                url_path: current_path,
                front_matter,
            })
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    let sitemap = SitemapNode::new(&pages);

    for page in pages {
        let mut ctx = tera::Context::new();
        ctx.try_insert("config", &config)?;
        ctx.try_insert("sitemap", &sitemap)?;
        ctx.try_insert("current_path", &page.url_path)?;
        ctx.try_insert("content", &page.rendered)?;

        if let Some(front_matter) = &page.front_matter {
            ctx.extend(
                tera::Context::from_serialize(front_matter)
                    .with_context(|| format!("Serialize front matter for {:?}", &page.url_path))?,
            );
        }

        let rendered = TERA
            .render("base.html", &ctx)
            .with_context(|| format!("Render template for {:?}", &page.url_path))?;

        all_assets.push(Asset::Page(Page {
            rendered,
            url_path: page.url_path,
            front_matter: page.front_matter,
        }));
    }

    for rel in static_relative_paths {
        let content = std::fs::read(config.docs_dir.join(&rel))
            .with_context(|| format!("Read static file {rel:?}"))?;

        all_assets.push(Asset::Static(StaticAsset {
            content,
            url_path: rel.to_string_lossy().into_owned(),
            mime_type: mime_guess::from_path(&rel).first_or_octet_stream(),
        }));
    }

    Ok(all_assets)
}
