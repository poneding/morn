//! 键盘快捷键相关纯函数。

/// 音量上调到下一个 5 的倍数(已是倍数则 +5), clamp 100。例: 43→45, 45→50。
pub fn snap_volume_up(vol: u8) -> u8 {
    (((vol / 5) + 1) * 5).min(100)
}

/// 音量下调到上一个 5 的倍数(已是倍数则 -5), clamp 0。例: 43→40, 45→40, 40→35。
pub fn snap_volume_down(vol: u8) -> u8 {
    (vol.saturating_sub(1) / 5) * 5
}

#[cfg(test)]
mod tests {
    use super::{snap_volume_down, snap_volume_up};

    #[test]
    fn up_snaps_to_next_multiple_of_5() {
        assert_eq!(snap_volume_up(43), 45);
        assert_eq!(snap_volume_up(45), 50);
        assert_eq!(snap_volume_up(40), 45);
        assert_eq!(snap_volume_up(98), 100);
        assert_eq!(snap_volume_up(100), 100);
    }

    #[test]
    fn down_snaps_to_prev_multiple_of_5() {
        assert_eq!(snap_volume_down(43), 40);
        assert_eq!(snap_volume_down(45), 40);
        assert_eq!(snap_volume_down(40), 35);
        assert_eq!(snap_volume_down(3), 0);
        assert_eq!(snap_volume_down(0), 0);
    }
}
