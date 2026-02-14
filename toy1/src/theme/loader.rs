//! Theme loading from embedded and external JSON files.

use std::path::Path;

use super::definition::ThemeDefinition;

const EMBEDDED_GREEN_SCREEN: &str = include_str!("../../themes/green-screen.json");
const EMBEDDED_DRACULA: &str = include_str!("../../themes/dracula.json");
const EMBEDDED_DEFAULT_DARK: &str = include_str!("../../themes/default-dark.json");

/// Load the themes embedded at compile time.
pub fn load_embedded_themes() -> Vec<ThemeDefinition> {
    [EMBEDDED_GREEN_SCREEN, EMBEDDED_DRACULA, EMBEDDED_DEFAULT_DARK]
        .iter()
        .filter_map(|src| serde_json::from_str::<ThemeDefinition>(src).ok())
        .collect()
}

/// Load themes from JSON files in a directory.
pub fn load_themes_from_dir(dir: &Path) -> Vec<ThemeDefinition> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                return None;
            }
            let content = std::fs::read_to_string(&path).ok()?;
            serde_json::from_str::<ThemeDefinition>(&content).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_themes_load() {
        let themes = load_embedded_themes();
        assert_eq!(themes.len(), 3);
    }

    #[test]
    fn embedded_slugs() {
        let themes = load_embedded_themes();
        let slugs: Vec<&str> = themes.iter().map(|t| t.slug.as_str()).collect();
        assert!(slugs.contains(&"green-screen"));
        assert!(slugs.contains(&"dracula"));
    }

    #[test]
    fn nonexistent_dir_returns_empty() {
        let themes = load_themes_from_dir(Path::new("/nonexistent"));
        assert!(themes.is_empty());
    }
}
