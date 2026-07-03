//! Windows 专属的窗口视觉调整。
//!
//! 对应 macOS 的 `layerContentsPlacement = TopLeft` 掩盖(见 `macos.rs`), Windows 侧
//! 的 resize/最大化旧帧拉伸掩盖是禁用 DWM 的窗口过渡动画
//! (`DWMWA_TRANSITIONS_FORCEDISABLED`)。
//!
//! DWM 在最大化/还原/最小化时播放约 200ms 的缩放过渡, 期间把窗口"当前内容"几何
//! 拉伸到过渡矩形上——旧尺寸帧被整幅放大, 自绘 UI 元素跟着被拉大再弹回, 即用户
//! 看到的"最大化时 UI 先拉伸、动画结束才恢复"。禁用过渡后窗口尺寸瞬间切换, 旧帧
//! 拉伸只剩下一次 present 之前的极短窗口(配合交换链 desired_maximum_frame_latency=1,
//! 通常 1 帧内), 肉眼基本不可见。全屏切换本就无 DWM 过渡动画, 不受影响。

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// 禁用当前窗口的 DWM 过渡动画; 返回 `true` 表示已成功应用, 调用方可据此只执行一次。
pub fn apply_resize_glitch_masking(frame: &eframe::Frame) -> bool {
    let Ok(handle) = frame.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return false;
    };
    disable_dwm_window_transitions(win32.hwnd.get())
}

fn disable_dwm_window_transitions(hwnd: isize) -> bool {
    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED,
    };

    let disabled: BOOL = 1;
    // SAFETY: hwnd 来自 winit 创建并持有的窗口, 在窗口生命周期内有效;
    // DwmSetWindowAttribute 只读取 pvattribute 指向的 4 字节 BOOL。
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            std::ptr::from_ref(&disabled).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )
    };
    result == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn resize_glitch_masking_disables_dwm_window_transitions() {
        let source = include_str!("windows.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("pub fn apply_resize_glitch_masking"));
        assert!(source.contains("DwmSetWindowAttribute"));
        assert!(source.contains("DWMWA_TRANSITIONS_FORCEDISABLED"));
        assert!(source.contains("RawWindowHandle::Win32"));
    }
}
