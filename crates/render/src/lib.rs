//! wgpu 视频纹理上传。注: wgpu 29 起类型名为 TexelCopyTextureInfo/TexelCopyBufferLayout
//! (旧名 ImageCopyTexture/ImageDataLayout 已移除)。
mod texture;
pub use texture::VideoTexture;
