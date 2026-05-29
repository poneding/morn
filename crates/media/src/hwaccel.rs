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
pub struct HwDeviceContext {
    ptr: *mut sys::AVBufferRef,
    /// 记录所选设备类型, 便于调试/日志; 当前 decoder 仅用 hw_pix_fmt 与指针。
    #[allow(dead_code)]
    pub device_type: sys::AVHWDeviceType,
    pub hw_pix_fmt: sys::AVPixelFormat,
}

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

/// 存入 AVCodecContext.opaque, 供 get_format 回调读取目标硬件格式。
pub struct HwCallbackData {
    pub hw_pix_fmt: sys::AVPixelFormat,
}

/// get_format 回调: 在候选格式列表中选我们的硬件格式; 找不到则回退首个(软)格式。
///
/// # Safety
/// 由 FFmpeg 在解码时调用。`ctx` 非空且其 `opaque` 指向有效的 HwCallbackData
/// (在 setup 时设置, 生命周期由 VideoDecoder 持有)。`fmt` 是以 AV_PIX_FMT_NONE
/// 结尾的有效格式数组指针。
pub unsafe extern "C" fn get_hw_format(
    ctx: *mut sys::AVCodecContext,
    fmt: *const sys::AVPixelFormat,
) -> sys::AVPixelFormat {
    let data = (*ctx).opaque as *const HwCallbackData;
    if data.is_null() {
        return *fmt; // 无数据, 返回第一个候选
    }
    let want = (*data).hw_pix_fmt;

    let mut p = fmt;
    while *p != sys::AVPixelFormat::AV_PIX_FMT_NONE {
        if *p == want {
            return want;
        }
        p = p.add(1);
    }
    // 未找到硬件格式: 返回第一个候选(通常软格式), 等效回退。
    *fmt
}

/// 把硬件帧下载到内存帧。成功返回 true。
///
/// # Safety
/// `hw_frame` 与 `sw_frame` 均为有效 AVFrame 指针; hw_frame 持有硬件表面,
/// sw_frame 为可写目标(可为空帧, transfer 会按需分配缓冲)。
pub unsafe fn transfer_hw_frame(
    hw_frame: *const sys::AVFrame,
    sw_frame: *mut sys::AVFrame,
) -> bool {
    // av_hwframe_transfer_data: dst, src, flags
    let ret = sys::av_hwframe_transfer_data(sw_frame, hw_frame, 0);
    if ret < 0 {
        return false;
    }
    // 拷贝 PTS 等元数据
    (*sw_frame).pts = (*hw_frame).pts;
    true
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
