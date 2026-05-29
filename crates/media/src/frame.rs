/// 解码并转换为 RGBA 后的视频帧, 可直接上传 GPU。
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub pts_ms: u64,
    pub rgba: Vec<u8>,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, pts_ms: u64, rgba: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pts_ms,
            rgba,
        }
    }
}

/// 解码后的一段音频, 交错 f32 样本。
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub pts_ms: u64,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>, channels: u16, pts_ms: u64) -> Self {
        Self {
            samples,
            channels,
            pts_ms,
        }
    }

    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / self.channels as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_frame_reports_dimensions_and_pts() {
        let f = VideoFrame::new(2, 2, 40, vec![0u8; 2 * 2 * 4]);
        assert_eq!(f.width, 2);
        assert_eq!(f.height, 2);
        assert_eq!(f.pts_ms, 40);
        assert_eq!(f.rgba.len(), 16);
    }

    #[test]
    fn audio_chunk_holds_interleaved_samples() {
        let c = AudioChunk::new(vec![0.1, -0.1, 0.2, -0.2], 2, 100);
        assert_eq!(c.channels, 2);
        assert_eq!(c.pts_ms, 100);
        assert_eq!(c.frame_count(), 2);
    }
}
