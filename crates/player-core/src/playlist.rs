use std::path::PathBuf;

pub struct Playlist {
    items: Vec<PathBuf>,
    cursor: usize,
}

impl Playlist {
    pub fn new() -> Self {
        Self { items: Vec::new(), cursor: 0 }
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

    fn p(s: &str) -> PathBuf { PathBuf::from(s) }

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
}
