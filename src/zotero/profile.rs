use sha2::Digest;

use crate::models::zotero::LibraryPaper;

/// Build collection paths from a flat list of Zotero collection objects.
///
/// Each collection has a key and a parentCollection. This function builds
/// slash-separated paths like "2026/survey/agent".
pub fn build_collection_paths(
    collections: &[serde_json::Value],
) -> std::collections::HashMap<String, String> {
    let mut name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut parent_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for col in collections {
        let Some(data) = col.get("data") else {
            continue;
        };
        let Some(key) = data.get("key").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(name) = data.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let parent = data
            .get("parentCollection")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        name_map.insert(key.to_string(), name.to_string());
        parent_map.insert(key.to_string(), parent.to_string());
    }

    let mut paths: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for key in name_map.keys() {
        let path = resolve_path(
            key,
            &name_map,
            &parent_map,
            &mut std::collections::HashSet::new(),
        );
        paths.insert(key.clone(), path);
    }

    paths
}

fn resolve_path(
    key: &str,
    name_map: &std::collections::HashMap<String, String>,
    parent_map: &std::collections::HashMap<String, String>,
    visited: &mut std::collections::HashSet<String>,
) -> String {
    if !visited.insert(key.to_string()) {
        return name_map.get(key).cloned().unwrap_or_default();
    }

    let name = name_map.get(key).cloned().unwrap_or_default();
    let parent = parent_map.get(key).cloned().unwrap_or_default();

    if parent.is_empty() || !name_map.contains_key(&parent) {
        name
    } else {
        let parent_path = resolve_path(&parent, name_map, parent_map, visited);
        if parent_path.is_empty() {
            name
        } else {
            format!("{}/{}", parent_path, name)
        }
    }
}

/// Compute a snapshot ID from the library state.
pub fn compute_snapshot_id(
    user_id: &str,
    library_version: Option<u64>,
    items: &[LibraryPaper],
) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(user_id.as_bytes());
    if let Some(v) = library_version {
        hasher.update(v.to_string().as_bytes());
    }
    for item in items {
        hasher.update(item.zotero_key.as_bytes());
        if let Some(v) = item.version {
            hasher.update(v.to_string().as_bytes());
        }
    }
    let result = hasher.finalize();
    hex::encode(result)
}

/// Compute the rules hash for filter configuration.
pub fn compute_rules_hash(filters: &[(String, f32, bool)]) -> String {
    let mut hasher = sha2::Sha256::new();
    for (path, weight, exclude) in filters {
        hasher.update(path.as_bytes());
        hasher.update(weight.to_le_bytes());
        hasher.update([*exclude as u8]);
    }
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_paths_simple() {
        let collections = vec![
            json!({"data": {"key": "A", "name": "parent", "parentCollection": ""}}),
            json!({"data": {"key": "B", "name": "child", "parentCollection": "A"}}),
        ];
        let paths = build_collection_paths(&collections);
        assert_eq!(paths.get("A").unwrap(), "parent");
        assert_eq!(paths.get("B").unwrap(), "parent/child");
    }

    #[test]
    fn build_paths_nested() {
        let collections = vec![
            json!({"data": {"key": "A", "name": "2026", "parentCollection": ""}}),
            json!({"data": {"key": "B", "name": "survey", "parentCollection": "A"}}),
            json!({"data": {"key": "C", "name": "agent", "parentCollection": "B"}}),
        ];
        let paths = build_collection_paths(&collections);
        assert_eq!(paths.get("C").unwrap(), "2026/survey/agent");
    }

    #[test]
    fn snapshot_id_deterministic() {
        let items = vec![];
        let id1 = compute_snapshot_id("user1", Some(100), &items);
        let id2 = compute_snapshot_id("user1", Some(100), &items);
        assert_eq!(id1, id2);
        assert!(!id1.is_empty());
    }
}
