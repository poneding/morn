# 设计: 播放内核重写 — 对标主流播放器 (ffplay/mpv 式)

> 状态: 关键决策已与用户对齐 (范围=方案 B 全量, 验证=HUD+分检查点)。待用户复审, 完成后转 writing-plans。
> 来源: 用户多轮实机反馈——增量修补后播放体验仍差(音画不同步、seek 慢、偶发卡顿)。结论: 当前架构是病灶, 需按主流播放器重写播放内核。
> 用户已明确: 这块技术细节由我把关, 用户只在每个检查点用调试 HUD / 手感验收。

## 背景: 为什么重写

当前是 **拉取式 + UI 驱动**:
- 视频呈现绑在 egui 的按需重绘上 → 队列偶发取空时丢失唤醒 → "播几秒卡一下"。
- A/V 同步判定 (`decide_frame`) 写在 UI 层 (`app/video_view`), 分层错误。
- 主时钟把**静音也计入播放时间** → 欠载/seek 空档时空转 → 漂移 + seek 后视频追逐前进的时钟 → "等很久"。
- 音频经时间拉伸器即使 1.0x 也常驻 → 固有延迟 → 画面稳定超前(此项已单独修)。
- 视频、音频**各自打开文件、各自 seek** → 双倍 I/O + 两个 seek 点。

主流播放器是 **推送式 + 时钟驱动**: 后台线程填帧队列, 稳定的呈现循环按主时钟挑帧, 时钟反映真实音频播放进度。本设计照此重写。

## 目标 / 成功标准

1. 正常播放**丝滑**, 无周期性卡顿/冻结(根治丢失唤醒)。
2. **音画同步**, 不漂移(主时钟反映真实音频播放, 不计静音)。
3. **seek 跟手**: 跳转后尽快出目标画面, 不追逐前进的时钟。
4. **暂停/恢复**干净, **倍速**(含变速不变调, 已具备)平滑, 视频随倍速正确跟随。
5. 纯视频(无音轨)文件能正常走时播放。
6. 保留全部现有功能与 egui 界面: 字幕、播放列表/历史、设置、硬解(VideoToolbox)+软解回退、截图、A-B、逐帧、续播。
7. 设计为**可扛 4K**(队列/缓冲按字节预算), 色彩转换先 CPU、留 GPU 接缝。

非目标(本次不做): GPU 色彩转换(NV12+着色器)留作二期; 位图字幕(PGS/VOBSUB); 倒放。

## 已确认决策

- **范围 = 方案 B**: 全量重写解复用→解码→同步→呈现内核; 含**统一单 demuxer**。保留 egui UI 与全部功能, 视频仍渲染到现有 egui/wgpu 画面, 但改由稳定呈现时钟驱动。
- **验证 = 调试 HUD + 分检查点**: 我无法亲见/听 eframe 窗口, 故做可开关 HUD 把"体验"量化, 并分 4 个检查点交付, 每个由用户实机验收(必要时回贴 HUD 数字)。
- **素材 = 混合(按 4K 设计)**: CPU 色彩转换 + 缓冲池, 留 GPU 接缝。主平台 macOS, 硬解 VideoToolbox, 保留软解回退。

## 架构: 线程与数据流

```
[Demuxer 线程] 独占 AVFormatContext(打开一次)
  loop: read packet → 按 stream index 分发 → video_pktq / audio_pktq(有界)
        处理 seek(单点 av_seek + flush + bump serial); 处理 EOF(发结束标记)
   │ video_pktq                              │ audio_pktq
   ▼                                         ▼
[视频解码线程] 独占 video codec ctx(!Send)    [音频解码线程] 独占 audio codec ctx + 重采样器
  pull pkt → send/receive → (硬解下载)         pull pkt → decode → 重采样到设备率
  → 缩放 RGBA(缓冲池复用)                       → (倍速≠1 时间拉伸; 1x 透传)
  → video_frameq(小有界 ~4 帧, 帧带 serial)    → 推样本到音频环形缓冲(带 PTS 锚)
   │                                          ▼
   │                                   [cpal 回调] = 主时钟: 只计真实样本→播放进度
   ▼
[呈现循环 @ app] 播放时 egui 连续重绘(贴 vsync):
   t = master_clock.now()
   引擎 present_frame(t): 从 video_frameq 丢迟帧/取到点帧/留未来帧, 丢弃 stale serial
   → 上传 wgpu 纹理 → 画
```

**有界队列做背压**: 帧队列满→视频解码阻塞→包队列满→demuxer 阻塞。内存可控。

**序号(serial)flush**: 管线持有 `serial: AtomicU64`。每次 seek `serial += 1`, demuxer 清空两包队列并(逻辑上)给后续包打新 serial; 解码线程遇到 serial 变化先 `avcodec_flush_buffers`; 帧带产生时的 serial; 呈现端/帧队列丢弃 `serial != current` 的残留。音频时钟也带 serial, 仅对当前 serial 累计。这是 ffplay 处理 flush 的成熟做法, 取代上一轮临时的 seek-gate 补丁, 无竞态。

## 主时钟与 A/V 同步

**主时钟(音频为主)** —— 媒体位置:
```
position_ms = anchor_pts_ms + (real_samples_played - anchor_samples) * 1000 / device_sample_rate * rate_pct / 100
```
- `real_samples_played`: cpal 回调里**只对真正从环形缓冲取到的样本**累加(取不到补静音但**不计数**)。→ 欠载/seek 空档时钟自动停住, 不空转。
- 锚点(`anchor_pts_ms`, `anchor_samples`)在 **开播 / seek / 倍速变更 / 恢复** 时重置为 (当前媒体位置, 当前 real_samples_played)。
- **暂停**: 暂停 cpal 流 → 回调停 → real_samples_played 冻结 → 位置冻结。
- 带 serial: seek 后旧 serial 的样本不计入(配合 flush)。

**墙钟回退(无音频轨)**: master = `std::time::Instant` 起点 + 已过墙钟 × rate(暂停冻结、seek 重锚)。使纯视频文件正常走时。

**A/V 同步(在引擎, 非 UI)**: 呈现循环每帧调 `present_frame(now_ms)`:
- 从 frameq 头部看帧: `pts < now - 阈值` → 丢(迟帧, 追赶); `|pts - now| ≤ 阈值` 或 `pts ≤ now` 的最新一帧 → 设为当前显示帧; `pts > now + 阈值` → 留队, 保持当前帧。
- 一次最多前进到"该显示的那帧", 不一次性耗尽队列(避免快进式跳播)。阈值取约半帧~一帧(按帧率, 缺省 ~10–20ms)。
- 防呆: 单次最多丢 N 帧(避免巨量丢帧时长时间黑屏), 超出则直接跳显最新可用帧。

**呈现循环(app, eframe)**: 播放态每帧 `ctx.request_repaint()`(连续重绘, 贴显示器刷新), 取 `present_frame` 上传纹理; 暂停/停止则不请求(静止)。彻底脱离"原子标志唤醒"模型 → 根治丢失唤醒。CPU: 仅播放时 ~60Hz UI 重绘, 可接受(主流播放器也按 vsync 呈现)。

## seek / 暂停 / 倍速

- **seek(ms)**: `serial += 1` → 通知 demuxer 单点 `av_seek_frame(≤target)` 并 flush 两包队列 → 解码线程 flush codec、丢旧 serial → 时钟 `anchor = (target, 当前 real_samples)` 并**闸住**(暂停音频流/冻结墙钟)直到"当前 serial 且 pts≥target 的首帧就绪"或 EOF, 再恢复。→ 视频不追逐前进的时钟, 等待收敛为"纯解码到目标"的耗时。闸门用 serial+首帧判定(取代旧的代次补丁)。
- **暂停/恢复**: 暂停=暂停音频流(冻结主时钟)+保留当前帧; 恢复=重锚+恢复流。停止=拆线程、清状态。
- **倍速 set_rate(pct)**: 重锚主时钟(当前位置, 新 rate)+ 音频 `PlaybackRateConverter`(1x 透传, ≠1 时间拉伸, 已实现); 视频按加速后的时钟选帧, 自然丢帧跟随。
- **逐帧 / A-B / 截图**: 基于新的 present/clock 重新接好(逐帧=暂停下推进一帧并锚定其 pts; 截图取当前显示帧)。

## 帧格式 / 吞吐 / 缓冲

- 帧仍为 **RGBA → egui native texture**(集成最简)。`media` 缩放输出 RGBA。
- **缓冲池**: 复用 `Vec<u8>`(或定长缓冲)避免每帧 ~8MB(1080p)/~33MB(4K)重分配。解码线程从池取、填、随帧发出; 呈现端上传后归还。池+帧队列按**字节预算**上限(如 ≤ ~256MB), 据分辨率推算帧数(4K 约 4–6 帧)。
- **GPU 接缝**: 帧抽象保留"像素数据 + 尺寸 + pts + serial", 将来可加 NV12 平面变体, 仅改 `render` 上传与 `media` 输出, 不动同步/时钟。

## 引擎 API 与 app 集成

`engine::Player`(重写, 对 app 暴露稳定接口):
- 控制: `open/play/pause/toggle/stop/seek_ms/set_rate/set_volume/toggle_mute/next/prev/...`(沿用现有 `Command` 枚举语义)。
- 查询: `timeline()`(位置由主时钟、时长、状态、rate、volume、muted); `current_video_dimensions()`; 字幕轨等。
- **呈现**: `present_frame() -> Option<FrameRef>`(引擎内部按主时钟选帧, 返回当前应显示帧给 UI 上传)。
- **统计**: `stats() -> PlaybackStats`(A/V 偏差、丢/迟帧累计、解码 fps、呈现 fps、各队列水位)——供 HUD。

`app/video_view`: 退化为"取 `present_frame` → 上传纹理 → 画 + 字幕叠加"; **移除 `decide_frame`**(同步逻辑回归引擎)。`app` update 循环: 播放态连续 `request_repaint`; 既有交互/缩放重绘逻辑保留。HUD 为可开关叠加层。

## 分层 / 文件边界

- `media`: `Demuxer`(独占 Input、读包、单点 seek、EOF); `VideoStreamDecoder`(包→RGBA 帧, 硬解+软解回退, 复用现有 hwaccel/scaler); `AudioStreamDecoder`(包→重采样样本)。**重构点**: 把现有"解码器自持 format ctx"拆成"Demuxer 给包 + 流解码器吃包"。
- `audio`: `MasterClock`(重写: 真实样本/倍速/欠载停住/serial); `AudioOutput`(cpal 回调只计真实样本); `PlaybackRateConverter`(已: 1x 透传)。
- `engine`: 线程编排(demuxer/视频/音频三线程 + 队列/环形缓冲)、`Player` API、A/V 同步选帧、seek/暂停/倍速、stats、EOF/结束动作(沿用 StopAtEnd/RepeatOne/LoopPlaylist)。
- `sync`: 选帧/时序纯函数(扩展现有 `decide_frame`: 选帧、丢帧上限、首帧闸门判定), 全单测。
- `render`: `VideoTexture`(+缓冲池辅助)。
- `app`: `video_view`(简化)、update(连续重绘)、HUD 叠加、控件接引擎。

## 错误处理

- 解码错误: 跳过该帧/包并记日志; 硬解打开失败→软解回退(保留现逻辑)。
- demuxer 读包 EOF: 发结束标记, 逐级排空后触发结束动作。读包错误: 终止该会话并上抛。
- 文件打开失败: 上抛给 UI 提示(现有 eprintln + 不崩)。
- 线程异常退出: 引擎检测到通道断开即停止会话, 不死锁(队列用带超时/断开感知的收发)。

## 测试策略

纯逻辑单测(高价值, 不依赖窗口):
- 时钟数学: 真实样本累计、倍速、欠载停住(静音不前进)、锚点重置、墙钟回退。
- 选帧: 迟帧丢/到点显/未来留、丢帧上限、阈值边界。
- 序号 flush: 旧 serial 包/帧被丢弃; seek 首帧闸门判定。
- EOF/结束动作判定。
集成测试: 用 `media/tests/fixtures/sample.mp4` 跑 Demuxer+解码线程(解码 N 帧、seek、EOF), 扩展现有 media/engine 测试。
运行手感: HUD + 用户分检查点验收(我无法亲验)。每检查点须 `cargo test --workspace` + `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` 全绿。

## 调试 HUD

可开关叠加层(快捷键, 如 Ctrl/Cmd+I 或某键): 显示 A/V 偏差(ms)、累计丢帧/迟帧、解码 fps、呈现 fps、video_frameq / 包队列 / 环形缓冲水位、当前 serial、主时钟来源(音频/墙钟)。数据来自 `Player::stats()`。默认关。

## 分检查点交付(实现顺序)

- **CP1 — 时序内核**: 新 `MasterClock`(真实样本+墙钟回退) + A/V 同步入引擎 + 呈现循环(连续重绘)。先在**现有两解码器结构**上做, 大概率即修好"卡顿/同步/暂停"手感。用户验收。
- **CP2 — 统一 demuxer + 序号 seek**: 引入单 Demuxer 线程 + 包队列 + serial flush, seek 走单点 + 首帧闸门。验收 seek 跟手。
- **CP3 — 吞吐/4K**: 缓冲池复用 + 按字节预算的队列 + 硬解路径核对。验收 4K/高码率流畅。
- **CP4 — HUD 收尾 + 功能回接**: HUD 完善; 逐帧/A-B/截图/续播/播放列表全部基于新内核回归。全量验收。

每个 CP 都是可运行、可验收的增量; 任一 CP 不达标即就地排查, 不堆叠到下一阶段。

## 已知风险 / 限制

- 我**看不到/听不到**运行效果 → 依赖 HUD + 用户验收; 故纯逻辑尽量单测、分检查点小步交付。
- 4K HEVC 若**硬解失败回退软解**, 单线程软解可能仍跟不上(物理限制); VideoToolbox 正常时应流畅。
- eframe 连续重绘期间 UI 以 ~60Hz 重排, CPU 略升(可接受); 暂停即停。
- ffmpeg-next 跨线程: Demuxer 产出的 `Packet` 可跨线程移动; 解码器 ctx 各自留在所属线程(满足 !Send)。
