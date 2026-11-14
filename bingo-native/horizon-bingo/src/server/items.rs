//! Item registry ids by name. The file has one item per line, and the line number (from 0) is the id.
//! Blank lines take a number but add no item, so the numbers stay those of the file.
use std::collections::HashMap;
use std::path::Path;

pub struct ItemIds {
    ids: HashMap<String, i32>,
}

impl ItemIds {
    /// Reads the names from `text`. A name listed twice keeps its first line number.
    pub fn from_names(text: &str) -> Self {
        let mut ids = HashMap::new();
        for (index, line) in text.lines().enumerate() {
            let name = line.trim();
            if !name.is_empty() {
                ids.entry(name.to_string()).or_insert(index as i32);
            }
        }
        Self { ids }
    }

    /// The id of `name`, or `None` if the file does not list it.
    pub fn id(&self, name: &str) -> Option<i32> {
        self.ids.get(name).copied()
    }

    /// Number of distinct items listed.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Reads the file at `path` with [`ItemIds::from_names`].
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Ok(Self::from_names(&text))
    }
}
