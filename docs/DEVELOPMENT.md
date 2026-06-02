# 开发环境配置指南

本项目涉及 C 库绑定（FFmpeg）和多媒体底层处理，开发环境需要配置 Clang 和 FFmpeg 相关路径。

## 1. 核心依赖项

无论什么系统，编译本项目都需要以下组件：

* **Rust**: Stable 版本。
* **LLVM/Clang**: 用于 `bindgen` 生成 Rust 绑定（尤其是 `media` 库依赖 FFmpeg 时）。
* **FFmpeg 开发头文件和库**: 必须可搜索到。

---

## 2. Windows 环境配置（推荐使用 vcpkg）

Windows 下建议使用 `vcpkg` 自动化管理 FFmpeg。

### 第一步：安装必备软件

1. **安装 LLVM**: [下载地址](https://github.com/llvm/llvm-project/releases)。安装后记下 `bin` 目录路径（如 `C:\Program Files\LLVM\bin`）。
2. **安装 vcpkg**: [官方仓库](https://github.com/microsoft/vcpkg)。
3. **安装 FFmpeg**:

    ```powershell
    vcpkg install ffmpeg:x64-windows
    ```

### 第二步：配置 Cargo 环境变量

由于每个开发者的安装路径可能不同，我们提供了两种配置方式：

#### 方案 A：系统环境变量 (推荐，无需修改代码仓库)

将以下路径加入系统环境变量（重启 IDE 后生效）：

* `LIBCLANG_PATH` = `C:\Program Files\LLVM\bin`
* `VCPKG_ROOT` = `C:\Users\<YourUser>\vcpkg`

说明：对于标准的 64 位 Windows 环境，`vcpkg` 会自动推导体系结构为 `x64-windows`。不需要手动配置 `VCPKG_DEFAULT_TRIPLET`。

#### 方案 B：本地配置文件 (灵活但需手动创建)

我们将 `.cargo/config.toml` 列入 `.gitignore`，你可以参考 `.cargo/config.toml.example` 创建你自己的本地配置：

1. 复制模板：`cp .cargo/config.toml.example .cargo/config.toml`
2. 修改其中的路径为你的实际物理路径。

---

## 3. macOS 配置

```bash
brew install llvm ffmpeg
# 之后可能需要设置 LIBCLANG_PATH，具体取决于 brew 安装路径
export LIBCLANG_PATH="/usr/local/opt/llvm/lib"
```

---

## 4. Linux (Ubuntu/Debian) 配置

```bash
sudo apt-get install clang ffmpeg libavcodec-dev libavformat-dev libavutil-dev ... # 详见 README.md
```

---

## 5. 项目结构说明

* `crates/app`: 应用入口。

* `crates/engine`: 核心播放逻辑。
* `crates/media`: FFmpeg 底层交互。
* ... (更多细节参阅各 crate 内的 README)
