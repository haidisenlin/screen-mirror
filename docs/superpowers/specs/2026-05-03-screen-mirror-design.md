# Screen Mirror — 无线投屏系统设计文档

## 概述

电脑端（macOS + Windows）捕获屏幕/窗口/区域及系统音频，通过局域网实时传输到接收端（Android TV / Mac Mini / Windows PC 连投影仪），实现低延迟、高画质的桌面级无线投屏。

**目标延迟**：端到端 20-58ms（局域网直连），含投影仪场景 36-108ms。

## 系统架构

```
发送端 (PC/Mac)                         接收端 (Android TV / Mac Mini / Win PC)
┌──────────────────┐                    ┌──────────────────────┐
│ Screen Capture   │                    │ Transport (RTP/UDP)  │
│ + Audio Capture  │                    │ + FEC Decode         │
│       ↓          │                    │       ↓              │
│ HW Encode        │                    │ Adaptive Jitter Buf  │
│ (H.264 + Opus)   │                    │       ↓              │
│       ↓          │     LAN/WiFi       │ HW Decode + Render   │
│ RTP + FEC        │ ──── UDP ────────▶ │ (VideoToolbox/MF/    │
│       ↓          │                    │  MediaCodec + Metal/ │
│ UDP Send         │ ◀── RTCP ──────── │  D3D11/Surface)      │
│                  │                    │       ↓              │
│ TCP Control ◀──────── TCP ──────────▶ │ Audio Output         │
│ + mDNS Browse    │                    │ + NSD/mDNS Register  │
└──────────────────┘                    └──────────────────────┘
```

## 接收端矩阵

| 属性 | Android TV | Mac Mini (投影仪) | Windows PC (投影仪) |
|------|-----------|-------------------|---------------------|
| 语言 | Kotlin + NDK (C++) | Rust | Rust |
| 视频解码 | MediaCodec (异步) | VideoToolbox | Media Foundation + DXVA2 |
| 视频渲染 | SurfaceView | Metal (CAMetalLayer) | D3D11 + DXGI SwapChain |
| 音频输出 | Oboe (NDK, Exclusive) | CoreAudio (128 samples) | WASAPI Shared 低延迟 |
| 音频解码 | libopus (NDK) | libopus (Rust FFI) | libopus (Rust FFI) |
| 服务发现 | NSD (Android 原生) | Bonjour (系统内置) | DNS-SD (Win10+ 原生) |
| mDNS 实现 | Android NSD API | mdns-sd crate | mdns-sd crate |

## 配对与连接

### 配对协议 (SPAKE2)

```
TV 端启动:
  1. 生成 6 位随机投屏码，全屏显示码 + 设备 IP
  2. mDNS 注册: "_screenmirror._tcp", TXT = { device_name, device_id }
  3. 监听 TCP 23501

PC 端连接:
  1. mDNS 浏览 / 手动输入 IP / 子网扫描 23501 端口
  2. 用户选择设备，输入投屏码
  3. TCP 连接目标
  4. SPAKE2 密钥交换 (投屏码为共享密码, RFC 9382)
  5. 协商成功 → 获得会话密钥 → 能力协商 → 开始 RTP 流

安全保护:
  - SPAKE2 零知识证明，不暴露码的任何信息
  - TV 端速率限制: 3 次失败 → 锁定 60 秒 → 自动刷新码
  - 控制消息用会话密钥 AES-128-GCM 加密
  - 投屏码空闲 5 分钟自动刷新，断开后立即刷新
```

### 设备发现 (三级回退)

1. **mDNS 自动发现** — 默认，局域网正常环境自动工作
2. **手动输入 IP** — mDNS 失败时回退，TV 端显示自身 IP
3. **子网扫描** — 扫描本子网 /24 的 23501 端口

### 抢投机制

```
状态: 用户A 正在投屏，用户B 输入正确投屏码

  → 用户A 的 PC 弹出通知 + 播放提示音:
    "有人请求投屏 (10 秒后自动让出)" [保持连接]
    
  → 10 秒内 A 点击保持 → 拒绝 B
  → 超时或 A 无响应 → 断开 A，连接 B，通知 A 已断开
  
超时可配置: 5 / 10 / 15 / 30 秒 (默认 10 秒)
心跳超时: 30 秒无心跳自动释放连接
```

## 屏幕捕获与编码

### 捕获层

| 平台 | API | Rust Crate | 输出 |
|------|-----|-----------|------|
| macOS | ScreenCaptureKit | screencapturekit-rs | IOSurface (GPU) |
| Windows | DXGI Desktop Duplication | windows-capture | ID3D11Texture2D (GPU) |

支持模式: 全屏镜像 / 选择窗口 / 自定义区域。
帧率控制: 屏幕无变化时不捕获（脏区域检测），有变化时目标 60fps。

### 编码层

| 平台 | 编码器 | 接口 |
|------|--------|------|
| macOS | VideoToolbox | VTCompressionSession |
| Windows (NVIDIA) | NVENC | NVIDIA Video Codec SDK FFI |
| Windows (Intel) | QuickSync | Media Foundation IMFTransform |
| Windows (AMD) | AMF | AMF SDK FFI |

编码参数:
- Codec: H.264 Main Profile (CABAC, 无 B 帧)
- Level: 4.1 (1080p@60fps)
- Rate Control: CBR
- GOP: I + P 帧，关键帧间隔 2 秒
- Tune: Ultra Low Latency
- 目标码率: 1080p 约 8-15 Mbps, 4K 约 25-40 Mbps

零拷贝 pipeline: 捕获输出 GPU 纹理 → 直接送硬件编码器 → 无 GPU→CPU 回读。

### 音频

| 平台 | 捕获 API | 编码 |
|------|----------|------|
| macOS | ScreenCaptureKit 音频流 | Opus 48kHz Stereo 128kbps, 10ms 帧 |
| Windows | WASAPI Loopback | 同上 |

## 传输协议

### RTP 打包

- 视频: RFC 6184 (H.264 RTP), FU-A 分片, 90kHz 时钟
- 音频: RFC 7587 (Opus RTP), 每包一帧, 48kHz 时钟
- MTU: 1400 bytes

### 音视频同步

- 主要: RTP Header Extension (RFC 6051) — 首帧即同步
- 辅助: RTCP SR 每秒一次 — 校准时钟漂移
- 策略: 音频为主时钟，视频追音频 (丢帧/重复帧对齐)
- 触发校正: 音视频 PTS 差 > 40ms

### 丢包恢复 (分级)

```
第一层: FEC (Reed-Solomon)
  - 每 N 个数据包附加 K 个冗余包
  - 自适应冗余率:
    丢包 < 1%  → 5%
    丢包 1-3%  → 15%
    丢包 > 3%  → 25% + 降码率

第二层: IDR 请求 (兜底)
  - FEC 无法恢复 + 连续丢包 > 50ms → 请求关键帧

第三层: 断线重连
  - 连续丢包 > 1s → 重建连接
```

### 自适应 Jitter Buffer

```
视频: 最小 5ms → 初始 15ms → 最大 60ms
音频: 最小 10ms → 初始 20ms → 最大 60ms
调整: 基于最近 100 包抖动统计动态调整
溢出: 丢弃最旧帧
```

## 发送端架构

### 线程模型

```
Main Thread
└── 托盘 UI (tao 事件循环)

Capture Thread [实时优先级]
└── 屏幕捕获 → GPU 纹理句柄 → [SPSC Ring ×3]

Encode Thread [实时优先级]
└── [Ring] → GPU 硬编码 → NAL → [SPSC Ring ×2]

Audio Thread [实时优先级]
└── 音频捕获 → Opus 编码 → [SPSC Ring ×4]

Network Thread [实时优先级, std::net 阻塞 UDP]
└── [Ring] → RTP 打包 → FEC → UDP send()
    + RTCP 接收 (select/poll)

Control Thread [Tokio, 普通优先级]
└── TCP 控制 + mDNS 浏览 + 配对 + 心跳
```

线程优先级: audio_thread_priority crate (Mozilla)。
实时路径通信: rtrb (SPSC 无锁 ring buffer)。
控制路径通信: crossbeam-channel。

### 系统托盘 UI

技术栈: tray-icon + tao + wry (系统 WebView 渲染面板)。

主面板功能:
- 输入投屏码 / 选择已发现设备
- 投屏模式选择 (全屏/窗口/区域)
- 投屏中状态 (分辨率/帧率/码率/延迟/丢包)
- 暂停/断开
- 设置 (超时时间、画质偏好)

抢投通知: 系统通知弹窗 + 提示音，倒计时 10 秒。

### Crate 结构

```
screen-mirror/
├── src/
│   ├── main.rs              # 入口, 托盘初始化
│   ├── capture/             # 屏幕+音频捕获 (平台 trait)
│   ├── encode/              # 硬件编码 (平台 trait)
│   ├── transport/           # RTP + FEC + RTCP + UDP
│   ├── pairing/             # SPAKE2 配对 + mDNS + 控制通道
│   ├── tray/                # 系统托盘 + WebView 面板
│   └── session/             # 会话生命周期 + 自适应控制
└── Cargo.toml
```

## Android TV 接收端

### 架构

```
网络层: Java NIO (DatagramChannel + SocketChannel)
解码层: MediaCodec 异步模式 → Surface 零拷贝
渲染层: SurfaceView (独立线程)
音频层: libopus (NDK) + Oboe (PerformanceMode::LowLatency, Exclusive)
同步层: RTP Header Extension 时间戳解析, 音频为主时钟
发现层: Android NSD API
热管理: PowerManager thermal 回调, 分级降级
```

热降级策略:
- THERMAL_STATUS_MODERATE → 限制 30fps
- THERMAL_STATUS_SEVERE → 降至 720p + 30fps
- THERMAL_STATUS_CRITICAL → 暂停投屏，通知 PC 端

## 桌面接收端 (Mac Mini / Windows PC → 投影仪)

### 架构

与发送端共享 Rust crate (transport/pairing/session)。

解码渲染:
- macOS: VideoToolbox (RealTime=true) → IOSurface → Metal CAMetalLayer
- Windows: Media Foundation (LowLatencyMode=true, DXVA2) → D3D11 Waitable SwapChain

音频输出:
- macOS: CoreAudio AudioUnit, 128 samples buffer (≈ 2.7ms)
- Windows: WASAPI Shared 低延迟模式 (IAudioClient3), ≈ 10-15ms

投影仪适配:
- 禁用 VSync (Immediate present) 减少 16ms 渲染延迟
- 分辨率检测: EDID 读取 + 校验 + 用户确认 + 默认 1080p 回退
- 能力协商中上报显示器原生分辨率，发送端匹配编码避免接收端缩放

## 能力协商

消息格式: JSON over TCP (会话密钥 AES-128-GCM 加密后传输)。

```json
{
  "device_type": "android_tv",
  "supported_codecs": ["h264_main", "h264_baseline"],
  "max_resolution": { "width": 3840, "height": 2160 },
  "max_fps": 60,
  "audio": { "codec": "opus", "sample_rate": 48000, "channels": 2 },
  "display": { "width": 1920, "height": 1080, "refresh_hz": 60, "hdr": false }
}
```

## 关键依赖

```toml
# 系统托盘 + UI
tray-icon, tao, wry

# 实时通信
rtrb                     # SPSC 无锁 ring buffer
crossbeam-channel        # 控制消息
audio_thread_priority    # 实时线程 (Mozilla)

# 安全
spake2                   # PAKE 配对协议

# 网络
mdns-sd                  # mDNS 服务发现 (纯 Rust)
tokio                    # 控制路径异步 I/O

# 编码
opus                     # 音频编码

# FEC
reed-solomon-erasure     # 前向纠错

# macOS
screencapturekit-rs      # 屏幕捕获
objc2, objc2-metal       # Metal 渲染 + VideoToolbox FFI

# Windows
windows-capture          # DXGI Desktop Duplication
windows-rs               # D3D11 / Media Foundation / WASAPI FFI
```

## 端到端延迟预算

| 环节 | 预期延迟 |
|------|----------|
| 屏幕捕获 | 3-16ms |
| 硬件编码 | 4-8ms |
| RTP 打包 + FEC | < 1ms |
| 网络传输 (LAN) | < 1ms |
| Jitter Buffer | 5-20ms |
| 硬件解码 | 5-10ms |
| 渲染 (原生 API) | 1-2ms |
| **总计 (不含投影仪)** | **~20-58ms** |
| 投影仪额外延迟 | 16-50ms |
| **总计 (含投影仪)** | **~36-108ms** |

## 学术论据索引

| 设计决策 | 支撑文献 |
|----------|----------|
| GPU 硬件编码 | arXiv:2511.18688 (VCIP 2025), NVIDIA NvPipe |
| H.264 Main Profile 兼容性 | ExoPlayer#1952, Android MediaCodecInfo |
| RTP/UDP 低延迟传输 | RFC 8834, arXiv:2310.03256 (IEEE COMST 2023) |
| Opus 低延迟音频 | RFC 6716, arXiv:1602.04845 |
| FEC 优于 IDR 请求 | arXiv:2001.07852 (DeepRS), arXiv:2305.12333 (Grace) |
| 自适应 Jitter Buffer | IEEE 7020318, WebRTC NetEQ |
| RFC 6051 同步 | IETF RFC 6051 (Rapid Synchronisation) |
| SPAKE2 安全配对 | RFC 9382, IETF draft-ietf-dnssd-pairing |
| mDNS 不可靠性 | RFC 9119 (Multicast over 802 Wireless) |
| Tokio 实时不适用 | tokio#2702, PostHog Tokio 延迟分析 |
| wgpu 性能开销 | wgpu#1685 (Metal backend 12ms overhead) |
| WASAPI Exclusive 问题 | Microsoft docs, nvda#15775 |
| EDID 不可靠 | Atlona KB01632, Extron EDID 文档 |
| 零拷贝 pipeline | ACM Computing Surveys 2022 (3512342) |
