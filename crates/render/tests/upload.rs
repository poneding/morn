use render::VideoTexture;

#[test]
fn creates_and_uploads_without_panicking() {
    // 申请一个 headless 适配器; 无 GPU 环境则跳过(CI 容器常见)。
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()));
    let Ok(adapter) = adapter else {
        eprintln!("无可用 GPU 适配器, 跳过测试");
        return;
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

    let mut tex = VideoTexture::new(&device, 4, 4);
    assert_eq!(tex.size(), (4, 4));

    let pixels = vec![255u8; 4 * 4 * 4];
    tex.upload(&queue, &pixels); // 不 panic 即通过

    // 尺寸变化时重建
    tex.ensure_size(&device, 8, 8);
    assert_eq!(tex.size(), (8, 8));
}
