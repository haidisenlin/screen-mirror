# PIN 验证匹配确认功能设计

**版本:** v0.51（基于 v0.5 主分支迭代）

## 概述

用户在 sender 端输入 6 位 PIN 后，自动向局域网内所有已发现 receiver 发送轻量级验证请求。匹配成功后显示目标设备名称，用户确认后才建立投屏连接。

## 动机

当前流程：输入 PIN → 直接连接。用户无法确认即将连接的是哪台设备。新流程增加一个确认步骤，避免误连。

## 流程

```
Sender:                              Receiver (Android TV):
输入 6 位 PIN                         启动时生成 PIN，显示在屏幕
    ↓ (300ms 防抖)
向所有 receiver POST /verify-pin  →   匹配则回复 200 + device_name
    ↓                                 不匹配回复 403
匹配成功：按钮变"连接到 XXX"
用户点击确认
    ↓
UiCommand::Connect { addr, pin }  →   正式配对连接
    ↓
PairingSuccess → ModeSelect → Streaming
```

## 1. Receiver 端 /verify-pin 接口

### 接口定义

- 路径：`POST /verify-pin`
- Request body：`{"pin": "123456"}`
- 匹配成功：`200 {"device_name": "客厅电视"}`
- 不匹配：`403 {}`

### 行为

- Receiver 启动时生成 6 位随机数字 PIN，显示在 UI 上
- HTTP server 复用现有 TCP 端口（用于后续投屏连接的同一端口）
- `/verify-pin` 是只读查询，不建立连接、不改变 receiver 状态
- 局域网信任环境，不做频率限制

### Android TV 实现

使用 NanoHTTPd 或 Ktor embedded server，在 receiver app 已有的网络监听基础上增加路由，几乎零额外开销。

## 2. Sender 端新增消息类型

```rust
// UiCommand 新增
VerifyPin { pin: String }

// BackendEvent 新增
PinMatched { device_name: String, addr: SocketAddr }
PinNotFound
```

### Backend 处理 VerifyPin

收到 `VerifyPin { pin }` 后：
1. 获取当前已发现的所有 receiver 地址列表
2. 并发向每台 receiver 发送 `POST /verify-pin`，超时 2 秒
3. 第一个返回 200 的 receiver → 发送 `BackendEvent::PinMatched { device_name, addr }`
4. 全部返回 403 或超时 → 发送 `BackendEvent::PinNotFound`

## 3. Sender UI 状态管理

### PinVerifyState 枚举

```rust
pub enum PinVerifyState {
    Idle,                                         // PIN < 6 位
    Debouncing { since: Instant, pin: String },   // 等待 300ms 防抖
    Verifying,                                    // 已发送请求，等待回复
    Matched { device_name: String, addr: SocketAddr },  // 匹配成功
    NotFound,                                     // 未找到设备
}
```

### IdleViewState 新增

```rust
pub pin_verify_state: PinVerifyState,
```

### 触发逻辑（AppCore 每帧检查）

1. `pin_input.len() == 6` 且 PIN 内容与上次验证不同 → 进入 `Debouncing { since: now(), pin }`
2. `pin_input.len() < 6` → 重置为 `Idle`
3. 处于 `Debouncing` 且已过 300ms → 发送 `UiCommand::VerifyPin`，切换到 `Verifying`
4. 收到 `BackendEvent::PinMatched` → 切换到 `Matched`
5. 收到 `BackendEvent::PinNotFound` → 切换到 `NotFound`
6. 用户修改 PIN 任何一位 → 重新从步骤 1 开始

### 按钮状态映射

| PinVerifyState | 按钮文字 | 样式 |
|---|---|---|
| Idle | "开始投屏" | 灰色禁用 |
| Debouncing | "开始投屏" | 灰色禁用 |
| Verifying | "匹配中..." | 灰色禁用 + 加载动画 |
| Matched { device_name } | "连接到 {device_name}" | 品牌蓝可点击 |
| NotFound | "未找到设备" | 红色文字禁用 |

## 4. IdleAction 修改

```rust
pub enum IdleAction {
    None,
    Connect { device_index: usize, pin: String },  // 保留（设备列表直接点击）
    ConnectMatched,  // 用户确认连接匹配到的设备
}
```

处理 `ConnectMatched`：从 `PinVerifyState::Matched { addr, device_name }` 取出信息，发送 `UiCommand::Connect { addr, pin }`。

## 5. 安全性

- PIN 不在 mDNS 网络广播中暴露
- PIN 仅在验证请求中点对点传输（局域网内）
- 用户必须看到 receiver 屏幕才能获得 PIN（物理安全）
- 连接前有用户确认步骤，防止误连

## 6. 不在范围内

- PIN 过期/刷新机制（后续版本）
- 多台 receiver 同 PIN 冲突处理（6 位数字碰撞概率极低）
- Receiver 端 Android TV 完整实现（本 spec 只定义接口协议，receiver 实现独立于 sender 代码库）
