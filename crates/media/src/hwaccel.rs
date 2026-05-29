/// 解码选项。
#[derive(Debug, Clone, Copy)]
pub struct DecodeOptions {
    /// 是否尝试硬件解码(失败自动回退软解)。
    pub try_hardware: bool,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self { try_hardware: true }
    }
}

/// 当前平台优先尝试的硬件设备类型名(传给 av_hwdevice_find_type_by_name)。
/// 不支持的平台返回 None。
// 任务 1 阶段仅被测试调用; 任务 2 的 create_for_current_platform 会引用它, 届时移除此 allow。
#[allow(dead_code)]
pub fn preferred_hw_name() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("videotoolbox")
    } else if cfg!(target_os = "windows") {
        Some("d3d11va")
    } else if cfg!(target_os = "linux") {
        Some("vaapi")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_type_is_nonempty_name() {
        if let Some(name) = preferred_hw_name() {
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn decode_options_default_prefers_hw() {
        let opts = DecodeOptions::default();
        assert!(opts.try_hardware);
    }
}
