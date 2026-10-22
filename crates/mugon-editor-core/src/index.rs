//! In-memory search index over all packs, with user overrides applied.

use crate::{Asset, Library, Pack};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub const OVERRIDES_FILE: &str = "overrides.json";

/// User edits that survive re-imports, keyed by asset id.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Override {
    pub category: Option<String>,
    #[serde(default)]
    pub add_tags: Vec<String>,
    #[serde(default)]
    pub remove_tags: Vec<String>,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Overrides(pub BTreeMap<String, Override>);

impl Overrides {
    pub fn load(root: &Path) -> Result<Self> {
        let file = root.join(OVERRIDES_FILE);
        if !file.is_file() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&std::fs::read_to_string(file)?)?)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        std::fs::write(root.join(OVERRIDES_FILE), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    fn apply(&self, asset: &mut Asset) {
        let Some(o) = self.0.get(&asset.id) else { return };
        if let Some(c) = &o.category {
            asset.category = c.clone();
        }
        asset.tags.retain(|t| !o.remove_tags.contains(t));
        for t in &o.add_tags {
            if !asset.tags.contains(t) {
                asset.tags.push(t.clone());
            }
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Query {
    pub q: String,
    pub pack: Option<String>,
    pub category: Option<String>,
    pub tag: Option<String>,
    pub kind: Option<String>,
    pub offset: usize,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult<'a> {
    pub total: usize,
    pub assets: Vec<&'a Asset>,
}

#[derive(Debug, Serialize, Clone)]
pub struct Facet {
    pub name: String,
    pub count: usize,
}

pub struct Index {
    pub root: std::path::PathBuf,
    pub packs: Vec<Pack>,
    pub overrides: Overrides,
    by_id: HashMap<String, (usize, usize)>,
    hidden: std::collections::HashSet<String>,
}

impl Index {
    pub fn load(library: &Library) -> Result<Self> {
        let mut packs = library.load_packs()?;
        let overrides = Overrides::load(&library.root)?;
        let mut by_id = HashMap::new();
        let mut hidden = std::collections::HashSet::new();
        for (pi, pack) in packs.iter_mut().enumerate() {
            for (ai, asset) in pack.assets.iter_mut().enumerate() {
                overrides.apply(asset);
                if overrides.0.get(&asset.id).is_some_and(|o| o.hidden) {
                    hidden.insert(asset.id.clone());
                }
                by_id.insert(asset.id.clone(), (pi, ai));
            }
        }
        Ok(Self { root: library.root.clone(), packs, overrides, by_id, hidden })
    }

    pub fn get(&self, id: &str) -> Option<&Asset> {
        let (p, a) = self.by_id.get(id)?;
        Some(&self.packs[*p].assets[*a])
    }

    pub fn iter(&self) -> impl Iterator<Item = &Asset> {
        self.packs.iter().flat_map(|p| p.assets.iter()).filter(|a| !self.hidden.contains(&a.id))
    }

    pub fn search(&self, query: &Query) -> SearchResult<'_> {
        let tokens: Vec<String> = query.q.split_whitespace().map(str::to_ascii_lowercase).collect();
        let matches: Vec<&Asset> = self
            .iter()
            .filter(|a| query.pack.as_deref().is_none_or(|p| p.is_empty() || a.pack == p))
            .filter(|a| query.category.as_deref().is_none_or(|c| c.is_empty() || a.category == c || a.category.starts_with(&format!("{c}/"))))
            .filter(|a| query.tag.as_deref().is_none_or(|t| t.is_empty() || a.tags.iter().any(|x| x == t)))
            .filter(|a| query.kind.as_deref().is_none_or(|k| k.is_empty() || format!("{:?}", a.kind).eq_ignore_ascii_case(k)))
            .filter(|a| {
                if tokens.is_empty() {
                    return true;
                }
                let name = a.name.to_ascii_lowercase();
                let cat = a.category.to_ascii_lowercase();
                let pack = a.pack.to_ascii_lowercase();
                tokens.iter().all(|t| name.contains(t) || cat.contains(t) || pack.contains(t) || a.tags.iter().any(|x| x.contains(t)))
            })
            .collect();
        let total = matches.len();
        let limit = query.limit.unwrap_or(usize::MAX);
        SearchResult { total, assets: matches.into_iter().skip(query.offset).take(limit).collect() }
    }

    pub fn categories(&self, pack: Option<&str>) -> Vec<Facet> {
        self.facet(pack, |a| vec![a.category.clone()])
    }

    pub fn tags(&self, pack: Option<&str>, min_count: usize) -> Vec<Facet> {
        self.facet(pack, |a| a.tags.clone()).into_iter().filter(|f| f.count >= min_count).collect()
    }

    pub fn pack_facets(&self) -> Vec<Facet> {
        self.packs
            .iter()
            .map(|p| Facet { name: p.id.clone(), count: p.assets.iter().filter(|a| !self.hidden.contains(&a.id)).count() })
            .collect()
    }

    fn facet(&self, pack: Option<&str>, f: impl Fn(&Asset) -> Vec<String>) -> Vec<Facet> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for a in self.iter().filter(|a| pack.is_none_or(|p| p.is_empty() || a.pack == p)) {
            for k in f(a) {
                *counts.entry(k).or_default() += 1;
            }
        }
        let mut v: Vec<Facet> = counts.into_iter().map(|(name, count)| Facet { name, count }).collect();
        v.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
        v
    }

    /// Short prose summary for priming an agent.
    pub fn summary(&self) -> String {
        let total = self.iter().count();
        let mut s = format!("{} packs, {} assets.\n", self.packs.len(), total);
        for p in &self.packs {
            let mut cats: BTreeMap<&str, usize> = BTreeMap::new();
            for a in &p.assets {
                *cats.entry(a.category.as_str()).or_default() += 1;
            }
            let cats: Vec<String> = cats.into_iter().map(|(c, n)| format!("{c} ({n})")).collect();
            s += &format!("- {} [{} assets]: {}\n", p.id, p.assets.len(), cats.join(", "));
        }
        s
    }
}
