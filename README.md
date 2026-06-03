# Morn

Morn 是一个用 Rust 构建的跨平台轻量视频播放器。项目目标是保持体积克制、启动快、播放流畅，同时保留日常播放需要的文件打开、播放控制、播放列表、字幕、倍速、截图和播放记忆。

## 功能

- 桌面 GUI：基于 `egui` / `eframe` / `wgpu`
- 媒体解码：基于 FFmpeg 绑定，优先支持主流视频格式
- 音频输出：基于 `cpal`，以音频时钟驱动画面同步
- 播放列表：打开文件时自动扫描同目录视频，也支持打开文件夹
- 字幕：支持外挂 `.srt` / `.ass`，并可切换内嵌字幕轨
- 增强：倍速播放、截图、播放历史、续播记忆
- 设置：语言、主题、快进步长、字幕字号等偏好持久化

## 项目结构

```text
crates/
  app/          egui 桌面应用入口，二进制名为 morn
  engine/       播放器编排层，连接解码、音频、字幕、持久化和状态
  media/        FFmpeg 解封装、音视频解码、字幕轨读取
  audio/        cpal 音频输出和主时钟
  render/       wgpu 视频纹理上传
  player-core/  播放命令、状态机、播放列表等纯逻辑
  subtitle/     字幕模型与 .srt/.ass 解析
  persist/      偏好与播放记忆存储
  sync/         音视频同步决策
  playground/   媒体管线实验入口
```

## 环境依赖

本项目涉及 C 库绑定（FFmpeg），需要安装 Rust 环境及相关开发库。详细的各平台（Windows/macOS/Linux）环境搭建手册请参考 [开发环境配置指南](docs/DEVELOPMENT.md)。

## 开发

运行桌面播放器：

```bash
cargo run -p app
```

生成媒体测试样本：

```bash
bash crates/media/tests/gen_fixture.sh
```

执行常用检查：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

构建 release 二进制：

```bash
cargo build --release -p app --bin morn
```

## CI 与发布

仓库包含两个 GitHub Actions workflow：

- `CI`：在 `master` push 和指向 `master` 的 pull request 上运行格式检查、Clippy 和测试。
- `Release`：在推送 `vX.Y.Z` 或 `vX.Y.Z-suffix` tag 时运行检查，并在 GitHub hosted runner 上构建/上传原生安装包：
  - Linux x86_64 / aarch64：`.deb`、`.rpm`、`.AppImage`
  - macOS x86_64 / aarch64：`.dmg`
  - Windows x86_64：`.msi`、NSIS `.exe`
  - Windows aarch64：NSIS `.exe`

Release workflow 也支持在 GitHub Actions 页面手动 `workflow_dispatch`，输入已有 tag 后会重新构建并用 `--clobber` 覆盖同名 release assets。

发布示例：

```bash
git tag v0.1.0
git push origin v0.1.0

git tag v0.1.0-alpha.1
git push origin v0.1.0-alpha.1
```

提交信息建议使用 Conventional Commits，例如：

```text
feat(app): add playlist history
fix(media): handle decode fallback
docs: update usage guide
```

## License

MIT
