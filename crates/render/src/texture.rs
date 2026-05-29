/// 持有一个 Rgba8Unorm 纹理, 支持每帧上传与按需重建。
pub struct VideoTexture {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
}

impl VideoTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = Self::create(device, width, height);
        Self {
            texture,
            width,
            height,
        }
    }

    fn create(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("video_frame"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// 帧尺寸变化时重建底层纹理。
    pub fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if (width, height) != (self.width, self.height) {
            self.texture = Self::create(device, width, height);
            self.width = width;
            self.height = height;
        }
    }

    /// 上传一帧 RGBA 像素 (长度须为 width*height*4)。
    pub fn upload(&mut self, queue: &wgpu::Queue, rgba: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.width),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn create_view(&self) -> wgpu::TextureView {
        self.texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}
