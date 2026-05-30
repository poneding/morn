/// 把 `path` 记入历史: 去重(移除已存在)、插到队首、截断到 `cap`。
pub fn push_history(history: &mut Vec<String>, path: &str, cap: usize) {
    history.retain(|p| p != path);
    history.insert(0, path.to_string());
    history.truncate(cap);
}

#[cfg(test)]
mod tests {
    use super::push_history;

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
}
