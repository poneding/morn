//! Windows 专属的窗口视觉调整。
//!
//! 对应 macOS 的 `layerContentsPlacement = TopLeft` 掩盖(见 `macos.rs`), Windows 侧
//! 的 resize/最大化旧帧拉伸掩盖是禁用 DWM 的窗口过渡动画
//! (`DWMWA_TRANSITIONS_FORCEDISABLED`)。
//!
//! DWM 在最大化/还原/最小化时播放约 200ms 的 缩放过渡, 期间把窗口"当前内容"几何
//! 拉伸到过渡矩形上——旧尺寸帧被整幅放大, 自绘 UI 元素跟着被拉大再弹回, 即用户
//! 看到的"最大化时 UI 先拉伸、动画结束才恢复"。禁用过渡后窗口尺寸瞬间切换, 旧帧
//! 拉伸只剩下一次 present 之前的极短窗口(配合交换链 desired_maximum_frame_latency=1,
//! 通常 1 帧内), 肉眼基本不可见。全屏切换本就无 DWM 过渡动画, 不受影响。
//!
//! Windows 11 另需把窗口设为 DWM 圆角(`DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND`)。
//! 仅靠 egui 在透明窗口上画圆角填充时, 圆角外的像素是 clear(透明=黑), 在底部两角
//! 露出黑色直角; 而 DWM 圆角在系统层裁剪窗口外形, 四角一致圆滑, 不再有黑角。Win10
//! 无此 API, 调用失败即退回 egui 自绘圆角。

use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// 首帧对当前窗口应用 Windows 视觉调整:
/// - 禁用 DWM 过渡动画(掩盖 maximize/restore 旧帧拉伸);
/// - Windows 11 设 DWM 圆角(掩盖透明窗口四角的黑色直角)。
///
/// 返回 `true` 表示已成功应用, 调用方可据此只执行一次。
pub fn apply_resize_glitch_masking(frame: &eframe::Frame) -> bool {
    let Ok(handle) = frame.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return false;
    };
    let hwnd = win32.hwnd.get();
    disable_dwm_window_transitions(hwnd) & enable_dwm_round_corners(hwnd)
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

fn enable_dwm_round_corners(hwnd: isize) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    // DWMWCP_ROUND = 2(圆角)。Win11 专属属性, Win10 上 DwmSetWindowAttribute 返回非 0,
    // 此次调用整体仍算成功(过渡已禁用)。
    let preference: i32 = DWMWCP_ROUND;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            std::ptr::from_ref(&preference).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    result == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn resize_glitch_masking_disables_dwm_transitions_and_enables_round_corners() {
        let source = include_str!("windows.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("pub fn apply_resize_glitch_masking"));
        // 禁用 DWM 过渡动画
        assert!(source.contains("DwmSetWindowAttribute"));
        assert!(source.contains("DWMWA_TRANSITIONS_FORCEDISABLED"));
        // Windows 11 DWM 圆角
        assert!(source.contains("DWMWA_WINDOW_CORNER_PREFERENCE"));
        assert!(source.contains("DWMWCP_ROUND"));
        assert!(source.contains("RawWindowHandle::Win32"));
    }
}
