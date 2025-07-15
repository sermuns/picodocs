use confique::Config;
use serde::Deserialize;
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum NavItem {
    Path(String),
    ExternalLink(BTreeMap<String, String>),
    Section(BTreeMap<String, Vec<NavItem>>),
}

#[derive(Config, Clone, Debug, Serialize)]
pub struct Conf {
    pub title: Option<String>,

    /// Used as favicon, among other places
    pub icon_path: Option<PathBuf>,

    pub description: Option<String>,

    /// Sitemap will only generate if this is a full/absolute URL e.g. https://www.example.com/
    #[config(default = "/")]
    pub base_url: String,

    #[config(default = "en")]
    pub language: String,

    /// Root directory of markdown documentation
    #[config(default = "docs")]
    pub docs_dir: PathBuf,

    /// Where to place rendered site files
    #[config(default = "public")]
    pub output_dir: PathBuf,

    /// Follow symbolic links when traversing the docs directory
    #[config(default = false)]
    pub follow_links: bool,

    /// (If defined) controls the sitemap structure
    pub nav: Option<Vec<NavItem>>,
}

pub type PartialConf = <Conf as Config>::Partial;
