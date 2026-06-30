//! Playlist history helpers.
//!
//! History is stored as newest-first path strings in preferences.  The helpers here
//! keep that storage policy small and deterministic: pushes de-duplicate existing
//! entries before inserting, cap the list length, and expose index-based removal for
//! the UI without letting the app duplicate vector surgery.

/// 把 `path` 记入历史: 去重(移除已存在)、插到队首、截断到 `cap`。
pub fn push_history(history: &mut Vec<String>, path: &str, cap: usize) {
    history.retain(|p| p != path);
    history.insert(0, path.to_string());
    history.truncate(cap);
}

pub fn remove_history_index(history: &mut Vec<String>, index: usize) -> Option<String> {
    if index < history.len() {
        Some(history.remove(index))
    } else {
        None
    }
}

pub fn clear_history(history: &mut Vec<String>) {
    history.clear();
}

#[cfg(test)]
mod tests {
    use super::{clear_history, push_history, remove_history_index};

    #[test]
    fn pushes_to_front_dedups_and_caps() {
        let mut h: Vec<String> = vec![];
        push_history(&mut h, "/a", 3);
        push_history(&mut h, "/b", 3);
        push_history(&mut h, "/a", 3);
        assert_eq!(h, vec!["/a".to_string(), "/b".to_string()]);
        push_history(&mut h, "/c", 3);
        push_history(&mut h, "/d", 3);
        assert_eq!(
            h,
            vec!["/d".to_string(), "/c".to_string(), "/a".to_string()]
        );
    }

    #[test]
    fn removes_history_item_by_index() {
        let mut h = vec!["/a".to_string(), "/b".to_string(), "/c".to_string()];

        assert_eq!(remove_history_index(&mut h, 1), Some("/b".to_string()));

        assert_eq!(h, vec!["/a".to_string(), "/c".to_string()]);
        assert_eq!(remove_history_index(&mut h, 99), None);
    }

    #[test]
    fn clears_history_items() {
        let mut h = vec!["/a".to_string(), "/b".to_string()];

        clear_history(&mut h);

        assert!(h.is_empty());
    }
}
