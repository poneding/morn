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

use ffmpeg_sys_next as sys;
use std::ffi::CString;
use std::ptr;

/// RAII 包装一个 AVBufferRef* 硬件设备上下文。
// device_type / hw_pix_fmt 在任务 4 由 VideoDecoder 读取; 此前无读者, 暂 allow, 届时移除。
#[allow(dead_code)]
pub struct HwDeviceContext {
    ptr: *mut sys::AVBufferRef,
    pub device_type: sys::AVHWDeviceType,
    pub hw_pix_fmt: sys::AVPixelFormat,
}

// 任务 2 仅提供封装; 任务 4 的 VideoDecoder 会调用 create/as_ptr, 届时这些 allow 失效并移除。
#[allow(dead_code)]
impl HwDeviceContext {
    /// 按平台名创建硬件设备上下文。失败返回 None(调用方回退软解)。
    pub fn create_for_current_platform() -> Option<Self> {
        let name = preferred_hw_name()?;
        let cname = CString::new(name).ok()?;

        // SAFETY: av_hwdevice_find_type_by_name 接受合法 C 字符串指针, 返回枚举值;
        // 无效名返回 AV_HWDEVICE_TYPE_NONE, 下面立即检查。
        let device_type = unsafe { sys::av_hwdevice_find_type_by_name(cname.as_ptr()) };
        if device_type == sys::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
            return None;
        }

        let mut ptr: *mut sys::AVBufferRef = ptr::null_mut();
        // SAFETY: 传入合法 out 指针 &mut ptr 与有效 device_type; 设备名/选项为 null
        // 表示默认设备, flags=0。返回 <0 表示失败, 此时 ptr 未被写入, 无需释放。
        let ret = unsafe {
            sys::av_hwdevice_ctx_create(&mut ptr, device_type, ptr::null(), ptr::null_mut(), 0)
        };
        if ret < 0 || ptr.is_null() {
            return None;
        }

        let hw_pix_fmt = hw_pix_fmt_for(device_type);
        Some(Self {
            ptr,
            device_type,
            hw_pix_fmt,
        })
    }

    /// 暴露底层指针供 decoder 设置到 AVCodecContext。
    pub fn as_ptr(&self) -> *mut sys::AVBufferRef {
        self.ptr
    }
}

impl Drop for HwDeviceContext {
    fn drop(&mut self) {
        // SAFETY: ptr 由 av_hwdevice_ctx_create 成功创建且仅在此释放一次;
        // av_buffer_unref 接受指向该指针的指针, 递减引用计数并将其置空。
        unsafe { sys::av_buffer_unref(&mut self.ptr) };
    }
}

// 由 create_for_current_platform 调用; 任务 4 接通整条链路后此 allow 失效并移除。
#[allow(dead_code)]
fn hw_pix_fmt_for(t: sys::AVHWDeviceType) -> sys::AVPixelFormat {
    use sys::AVHWDeviceType::*;
    use sys::AVPixelFormat::*;
    match t {
        AV_HWDEVICE_TYPE_VIDEOTOOLBOX => AV_PIX_FMT_VIDEOTOOLBOX,
        AV_HWDEVICE_TYPE_D3D11VA => AV_PIX_FMT_D3D11,
        AV_HWDEVICE_TYPE_VAAPI => AV_PIX_FMT_VAAPI,
        _ => AV_PIX_FMT_NONE,
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
