use std::path::PathBuf;

pub struct Playlist {
    items: Vec<PathBuf>,
    cursor: usize,
}

impl Playlist {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            cursor: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn add(&mut self, path: PathBuf) {
        self.items.push(path);
    }

    pub fn current(&self) -> Option<&PathBuf> {
        self.items.get(self.cursor)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, std::path::PathBuf> {
        self.items.iter()
    }

    pub fn as_slice(&self) -> &[PathBuf] {
        &self.items
    }

    pub fn current_index(&self) -> Option<usize> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.cursor)
        }
    }

    pub fn set_cursor(&mut self, i: usize) {
        if i < self.items.len() {
            self.cursor = i;
        }
    }

    pub fn remove_index(&mut self, index: usize) -> Option<PathBuf> {
        if index >= self.items.len() {
            return None;
        }

        let removed = self.items.remove(index);
        if self.items.is_empty() {
            self.cursor = 0;
        } else if index < self.cursor {
            self.cursor -= 1;
        } else if index == self.cursor {
            self.cursor = index.min(self.items.len() - 1);
        }
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.cursor = 0;
    }

    /// 用新条目替换整个列表, cursor 收敛到 [0, len)。
    pub fn set_items(&mut self, items: Vec<std::path::PathBuf>, cursor: usize) {
        self.cursor = if items.is_empty() {
            0
        } else {
            cursor.min(items.len() - 1)
        };
        self.items = items;
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&PathBuf> {
        if self.cursor + 1 < self.items.len() {
            self.cursor += 1;
            self.current()
        } else {
            None
        }
    }

    pub fn prev(&mut self) -> Option<&PathBuf> {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.current()
        } else {
            None
        }
    }
}

impl Default for Playlist {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn empty_has_no_current() {
        let pl = Playlist::new();
        assert!(pl.current().is_none());
        assert_eq!(pl.len(), 0);
    }

    #[test]
    fn add_sets_first_as_current() {
        let mut pl = Playlist::new();
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        assert_eq!(pl.current(), Some(&p("/a.mp4")));
        assert_eq!(pl.len(), 2);
    }

    #[test]
    fn next_advances_and_stops_at_end() {
        let mut pl = Playlist::new();
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        assert_eq!(pl.next(), Some(&p("/b.mp4")));
        assert_eq!(pl.next(), None);
        assert_eq!(pl.current(), Some(&p("/b.mp4")));
    }

    #[test]
    fn prev_goes_back_and_stops_at_start() {
        let mut pl = Playlist::new();
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        pl.next();
        assert_eq!(pl.prev(), Some(&p("/a.mp4")));
        assert_eq!(pl.prev(), None);
    }

    #[test]
    fn next_on_empty_is_none() {
        let mut pl = Playlist::new();
        assert_eq!(pl.next(), None);
    }

    #[test]
    fn iter_index_and_set_cursor() {
        let mut pl = Playlist::new();
        assert_eq!(pl.current_index(), None);
        pl.add(p("/a.mp4"));
        pl.add(p("/b.mp4"));
        pl.add(p("/c.mp4"));
        assert_eq!(pl.iter().count(), 3);
        assert_eq!(pl.current_index(), Some(0));
        pl.set_cursor(2);
        assert_eq!(pl.current_index(), Some(2));
        assert_eq!(pl.current(), Some(&p("/c.mp4")));
        // 越界索引被忽略, 游标保持不变。
        pl.set_cursor(9);
        assert_eq!(pl.current_index(), Some(2));
    }

    #[test]
    fn set_items_replaces_and_sets_cursor() {
        let mut pl = Playlist::new();
        pl.add(p("/old.mp4"));
        pl.set_items(vec![p("/a.mp4"), p("/b.mp4"), p("/c.mp4")], 2);
        assert_eq!(pl.len(), 3);
        assert_eq!(pl.current(), Some(&p("/c.mp4")));
        pl.set_items(vec![p("/x.mp4")], 99); // 越界收敛末尾
        assert_eq!(pl.current(), Some(&p("/x.mp4")));
    }

    #[test]
    fn remove_index_keeps_cursor_on_same_logical_item_when_possible() {
        let mut pl = Playlist::new();
        pl.set_items(vec![p("/a.mp4"), p("/b.mp4"), p("/c.mp4")], 1);

        assert_eq!(pl.remove_index(0), Some(p("/a.mp4")));

        assert_eq!(pl.current_index(), Some(0));
        assert_eq!(pl.current(), Some(&p("/b.mp4")));
        assert_eq!(
            pl.iter().cloned().collect::<Vec<_>>(),
            vec![p("/b.mp4"), p("/c.mp4")]
        );
    }

    #[test]
    fn remove_current_prefers_next_then_previous() {
        let mut pl = Playlist::new();
        pl.set_items(vec![p("/a.mp4"), p("/b.mp4"), p("/c.mp4")], 1);

        assert_eq!(pl.remove_index(1), Some(p("/b.mp4")));
        assert_eq!(pl.current_index(), Some(1));
        assert_eq!(pl.current(), Some(&p("/c.mp4")));

        assert_eq!(pl.remove_index(1), Some(p("/c.mp4")));
        assert_eq!(pl.current_index(), Some(0));
        assert_eq!(pl.current(), Some(&p("/a.mp4")));
    }

    #[test]
    fn remove_last_item_leaves_empty_playlist() {
        let mut pl = Playlist::new();
        pl.set_items(vec![p("/a.mp4")], 0);

        assert_eq!(pl.remove_index(0), Some(p("/a.mp4")));

        assert_eq!(pl.len(), 0);
        assert_eq!(pl.current_index(), None);
        assert_eq!(pl.current(), None);
    }

    #[test]
    fn clear_removes_all_items_and_current_selection() {
        let mut pl = Playlist::new();
        pl.set_items(vec![p("/a.mp4"), p("/b.mp4")], 1);

        pl.clear();

        assert_eq!(pl.len(), 0);
        assert_eq!(pl.current_index(), None);
        assert_eq!(pl.current(), None);
    }
}
