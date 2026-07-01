# Morn 跨平台构建入口 (GNU Make, 兼容 macOS / Linux / Windows)
#
# Windows 用法: 先装好 make (scoop install make / choco install make / MSYS2 pacman -S make),
# 确保 PATH 里能找到 make、cargo、ffmpeg。之后:
#   make test      # 生成 fixture 并跑全工作区测试
#   make build     # 调试构建
#   make run       # 构建并启动 morn
#   make lint      # clippy (对齐 CI)
#   make check     # fmt + clippy + test (CI 完整门禁)
#
# 二进制名为 morn (crates/app/Cargo.toml 的 [[bin]] name = "morn")。

# --- 工具与标志 ---------------------------------------------------------------
CARGO ?= cargo
# 与 .github/workflows/ci.yml 对齐: --locked 保证用 Cargo.lock, --workspace 覆盖全部 crate。
CARGO_LOCKED ?= --locked
# Windows(mingw) 上 sh 可能不存在, 优先用 sh 生成 fixture; 无 sh 时跳过并提示。
SH ?= sh

# media 集成测试依赖 ffmpeg 生成的 mp4 fixture, 脚本本身是 bash。CARGO_MANIFEST_DIR 在
# 编译期展开, fixture 路径跨平台一致, 故只要能跑 gen_fixture.sh 即可。
FIXTURE_SCRIPT := crates/media/tests/gen_fixture.sh

# --- 默认目标 -----------------------------------------------------------------
.DEFAULT_GOAL := help

.PHONY: help fixture fmt lint test check build build-release run clean

help: ## 显示本帮助
	@awk 'BEGIN {FS = ":.*##"; printf "用法: make <target>\n\n目标:\n"} \
	  /^[a-zA-Z_-]+:.*##/ { printf "  %-16s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# --- 测试 fixture -------------------------------------------------------------
fixture: ## 生成 media 测试用的 mp4 样本 (需要 ffmpeg)
	@if command -v $(SH) >/dev/null 2>&1; then \
		$(SH) $(FIXTURE_SCRIPT); \
	else \
		echo "[fixture] 未找到 $(SH), 跳过样本生成。" >&2; \
		echo "[fixture] Windows 请用 Git Bash / MSYS2 提供 sh, 或手动跑 ffmpeg 见 $(FIXTURE_SCRIPT)。" >&2; \
	fi

# --- 代码质量 ----------------------------------------------------------------
fmt: ## 用 rustfmt 格式化全部代码
	$(CARGO) fmt --all

fmt-check: ## 仅检查格式不改动 (CI 用)
	$(CARGO) fmt --all -- --check

lint: ## clippy 全工作区 (对齐 CI: --workspace --all-targets)
	$(CARGO) clippy $(CARGO_LOCKED) --workspace --all-targets -- -D warnings

# --- 测试 ---------------------------------------------------------------------
test: fixture ## 生成 fixture 并跑全工作区测试 (对齐 CI)
	$(CARGO) test $(CARGO_LOCKED) --workspace

# check = CI 完整门禁: 格式 + lint + 测试。本地提交前跑一遍即可复刻 CI 结果。
check: fmt-check lint test ## CI 完整门禁: fmt + clippy + test

# --- 构建 ---------------------------------------------------------------------
build: ## 调试构建 (dev profile)
	$(CARGO) build $(CARGO_LOCKED) --workspace

# release profile 在根 Cargo.toml 已配 strip + thin LTO + codegen-units=1,
# 构建慢但产物小, 用于发版。Windows 产物为 target/release/morn.exe。
build-release: ## Release 构建 (LTO + strip)
	$(CARGO) build $(CARGO_LOCKED) --workspace --release

# --- 运行 ---------------------------------------------------------------------
# cargo run 会自动选 morn 这个 [[bin]]; --bin morn 显式指定更稳, 避免 workspace 多 bin 时歧义。
run: ## 构建并启动 morn (dev profile)
	$(CARGO) run $(CARGO_LOCKED) --bin morn

# --- 清理 ---------------------------------------------------------------------
clean: ## 删除 target/ 与 media 测试 fixture
	$(CARGO) clean
	@rm -rf crates/media/tests/fixtures
