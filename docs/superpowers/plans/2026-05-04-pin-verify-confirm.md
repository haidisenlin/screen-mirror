# PIN 验证匹配确认 Implementation Plan (v0.51)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user inputs a 6-digit PIN, auto-verify it against all discovered receivers via HTTP, display the matched device name, and require explicit confirmation before connecting.

**Architecture:** Add `VerifyPin`/`PinMatched`/`PinNotFound` messages. Backend sends concurrent `POST /verify-pin` to all discovered receivers. UI tracks `PinVerifyState` with 300ms debounce, shows match result on the connect button. Receiver exposes a `/verify-pin` HTTP endpoint.

**Tech Stack:** Rust, `ureq` (blocking HTTP client), `serde_json`, eframe/egui

---

## File Structure

| File | Responsibility |
|---|---|
| `Cargo.toml` | Add `ureq` dependency |
| `src/ui/messages.rs` | Add `VerifyPin`, `PinMatched`, `PinNotFound` message variants |
| `src/ui/views/idle.rs` | Add `PinVerifyState` enum, update `IdleViewState`, update button rendering, change `IdleAction` |
| `src/ui/app.rs` | Add debounce checking, handle new backend events, handle `ConnectMatched` action |
| `src/ui/backend.rs` | Handle `VerifyPin` command — HTTP POST to all receivers |

---

### Task 1: Add `ureq` dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add ureq to dependencies**

In `Cargo.toml`, add to the `[dependencies]` section:

```toml
ureq = { version = "3", features = ["json"] }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add ureq for HTTP client"
```

---

### Task 2: Add new message types

**Files:**
- Modify: `src/ui/messages.rs`
- Test: `src/ui/messages.rs` (inline tests)

- [ ] **Step 1: Write the failing tests**

Add these tests to the existing `#[cfg(test)] mod tests` in `src/ui/messages.rs`:

```rust
#[test]
fn verify_pin_command_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<UiCommand>();
}

#[test]
fn pin_matched_event_debug() {
    let event = BackendEvent::PinMatched {
        device_name: "客厅电视".to_string(),
        addr: "192.168.1.100:9000".parse().unwrap(),
    };
    let _ = format!("{:?}", event);
}

#[test]
fn pin_not_found_event_debug() {
    let event = BackendEvent::PinNotFound;
    let _ = format!("{:?}", event);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::messages::tests`
Expected: FAIL — `PinMatched` and `PinNotFound` variants do not exist

- [ ] **Step 3: Add the new variants**

In `src/ui/messages.rs`, add to the `UiCommand` enum:

```rust
VerifyPin { pin: String },
```

Add to the `BackendEvent` enum:

```rust
PinMatched { device_name: String, addr: std::net::SocketAddr },
PinNotFound,
```

- [ ] **Step 4: Update existing exhaustive tests**

In the `all_backend_events_debug` test, add the two new variants to the `events` vec:

```rust
BackendEvent::PinMatched {
    device_name: "test".to_string(),
    addr: "127.0.0.1:9000".parse().unwrap(),
},
BackendEvent::PinNotFound,
```

In the `all_ui_commands_debug` test, add to the `commands` vec:

```rust
UiCommand::VerifyPin {
    pin: "123456".to_string(),
},
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib ui::messages::tests`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/ui/messages.rs
git commit -m "feat: add VerifyPin/PinMatched/PinNotFound message types"
```

---

### Task 3: Add `PinVerifyState` and update `IdleViewState`

**Files:**
- Modify: `src/ui/views/idle.rs`

- [ ] **Step 1: Add `PinVerifyState` enum and update `IdleViewState`**

At the top of `src/ui/views/idle.rs`, after the existing imports, add:

```rust
use std::net::SocketAddr;
use std::time::Instant;
```

Before the `IdleViewState` struct, add:

```rust
#[derive(Debug)]
pub enum PinVerifyState {
    Idle,
    Debouncing { since: Instant, pin: String },
    Verifying,
    Matched { device_name: String, addr: SocketAddr },
    NotFound,
}

impl Default for PinVerifyState {
    fn default() -> Self {
        Self::Idle
    }
}
```

Add a new field to `IdleViewState`:

```rust
pub pin_verify_state: PinVerifyState,
```

- [ ] **Step 2: Update all `IdleViewState` construction sites**

In `src/ui/app.rs`, in `AppCore::new()`, add to the `IdleViewState` initialization:

```rust
pin_verify_state: PinVerifyState::Idle,
```

This requires adding `use crate::ui::views::idle::PinVerifyState;` to the imports at the top of `app.rs`.

- [ ] **Step 3: Run tests to verify compilation**

Run: `cargo test`
Expected: all tests PASS (no logic changes yet)

- [ ] **Step 4: Commit**

```bash
git add src/ui/views/idle.rs src/ui/app.rs
git commit -m "feat: add PinVerifyState enum and field to IdleViewState"
```

---

### Task 4: Update connect button to reflect `PinVerifyState`

**Files:**
- Modify: `src/ui/views/idle.rs`

- [ ] **Step 1: Replace the connect button logic**

In `src/ui/views/idle.rs`, find the current connect button section (starts with `// Connect button`). Replace it entirely:

```rust
    // Connect button
    ui.horizontal(|ui| {
        ui.add_space(PADDING);
        let btn_width = PANEL_WIDTH - PADDING * 2.0 - 4.0;

        let (btn_text, text_color, fill, stroke, enabled) = match &state.pin_verify_state {
            PinVerifyState::Matched { device_name, .. } => (
                format!("连接到 {device_name}"),
                COLOR_BG_WHITE,
                COLOR_BRAND,
                Stroke::NONE,
                true,
            ),
            PinVerifyState::Verifying => (
                "匹配中...".to_string(),
                COLOR_MUTED,
                COLOR_BG_CARD,
                Stroke::new(1.0, COLOR_BORDER),
                false,
            ),
            PinVerifyState::NotFound => (
                "未找到设备".to_string(),
                COLOR_ERROR,
                COLOR_BG_CARD,
                Stroke::new(1.0, COLOR_BORDER),
                false,
            ),
            _ => (
                "开始投屏".to_string(),
                COLOR_MUTED,
                COLOR_BG_CARD,
                Stroke::new(1.0, COLOR_BORDER),
                false,
            ),
        };

        let btn = Button::new(RichText::new(&btn_text).size(15.0).strong().color(text_color))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(BUTTON_ROUNDING))
            .min_size(Vec2::new(btn_width, BUTTON_HEIGHT));

        if ui.add_enabled(enabled, btn).clicked() {
            if matches!(state.pin_verify_state, PinVerifyState::Matched { .. }) {
                action = IdleAction::ConnectMatched;
            }
        }
    });
```

- [ ] **Step 2: Update `IdleAction` enum**

In `src/ui/views/idle.rs`, replace `ConnectAny` with `ConnectMatched`:

```rust
pub enum IdleAction {
    None,
    Connect { device_index: usize, pin: String },
    ConnectMatched,
}
```

- [ ] **Step 3: Run to verify compilation**

Run: `cargo check`
Expected: may fail in `app.rs` due to removed `ConnectAny` — that's expected, will fix in Task 5

- [ ] **Step 4: Commit**

```bash
git add src/ui/views/idle.rs
git commit -m "feat: connect button reflects PinVerifyState"
```

---

### Task 5: Handle `PinVerifyState` in `AppCore` (debounce + events + ConnectMatched)

**Files:**
- Modify: `src/ui/app.rs`
- Test: `src/ui/app.rs` (inline tests)

- [ ] **Step 1: Write failing tests**

Add these tests to the existing `#[cfg(test)] mod tests` in `src/ui/app.rs`:

```rust
#[test]
fn pin_verify_debounce_starts_on_6_digits() {
    let (core, _rx) = make_core();
    core.idle_view.pin_input = "123456".to_string();
    core.check_pin_verify();
    assert!(matches!(
        core.idle_view.pin_verify_state,
        PinVerifyState::Debouncing { .. }
    ));
}

#[test]
fn pin_verify_resets_on_short_pin() {
    let (core, _rx) = make_core();
    core.idle_view.pin_input = "123456".to_string();
    core.check_pin_verify();
    core.idle_view.pin_input = "12345".to_string();
    core.check_pin_verify();
    assert!(matches!(
        core.idle_view.pin_verify_state,
        PinVerifyState::Idle
    ));
}

#[test]
fn pin_matched_event_updates_state() {
    let (core, _rx) = make_core();
    let addr: std::net::SocketAddr = "192.168.1.100:9000".parse().unwrap();
    core.idle_view.pin_verify_state = PinVerifyState::Verifying;
    // Simulate backend sending PinMatched
    core.event_rx = {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(BackendEvent::PinMatched {
            device_name: "客厅电视".to_string(),
            addr,
        }).unwrap();
        rx
    };
    core.process_backend_events();
    assert!(matches!(
        core.idle_view.pin_verify_state,
        PinVerifyState::Matched { ref device_name, .. } if device_name == "客厅电视"
    ));
}

#[test]
fn pin_not_found_event_updates_state() {
    let (core, _rx) = make_core();
    core.idle_view.pin_verify_state = PinVerifyState::Verifying;
    core.event_rx = {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(BackendEvent::PinNotFound).unwrap();
        rx
    };
    core.process_backend_events();
    assert!(matches!(
        core.idle_view.pin_verify_state,
        PinVerifyState::NotFound
    ));
}

#[test]
fn connect_matched_sends_command() {
    let (core, rx) = make_core();
    let addr: std::net::SocketAddr = "192.168.1.100:9000".parse().unwrap();
    core.idle_view.pin_input = "123456".to_string();
    core.idle_view.pin_verify_state = PinVerifyState::Matched {
        device_name: "客厅电视".to_string(),
        addr,
    };
    core.handle_connect_matched();
    let cmd = rx.try_recv().unwrap();
    assert!(matches!(cmd, UiCommand::Connect { .. }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ui::app::tests`
Expected: FAIL — methods don't exist yet

- [ ] **Step 3: Add `check_pin_verify` method to AppCore**

In `src/ui/app.rs`, add this method to `impl AppCore`:

```rust
fn check_pin_verify(&mut self) {
    let pin = &self.idle_view.pin_input;

    if pin.len() < 6 {
        self.idle_view.pin_verify_state = PinVerifyState::Idle;
        return;
    }

    match &self.idle_view.pin_verify_state {
        PinVerifyState::Debouncing { pin: prev_pin, since } => {
            if prev_pin != pin {
                self.idle_view.pin_verify_state = PinVerifyState::Debouncing {
                    since: Instant::now(),
                    pin: pin.clone(),
                };
            } else if since.elapsed() >= Duration::from_millis(300) {
                let _ = self.cmd_tx.send(UiCommand::VerifyPin { pin: pin.clone() });
                self.idle_view.pin_verify_state = PinVerifyState::Verifying;
            }
        }
        PinVerifyState::Matched { .. } | PinVerifyState::NotFound => {
            // If PIN changed, re-debounce
            let prev_pin = match &self.idle_view.pin_verify_state {
                PinVerifyState::Matched { .. } | PinVerifyState::NotFound => None,
                _ => unreachable!(),
            };
            if prev_pin.is_none() {
                // Can't compare, just re-debounce
                self.idle_view.pin_verify_state = PinVerifyState::Debouncing {
                    since: Instant::now(),
                    pin: pin.clone(),
                };
            }
        }
        PinVerifyState::Idle => {
            self.idle_view.pin_verify_state = PinVerifyState::Debouncing {
                since: Instant::now(),
                pin: pin.clone(),
            };
        }
        PinVerifyState::Verifying => {
            // Wait for response
        }
    }
}
```

- [ ] **Step 4: Add `handle_connect_matched` method to AppCore**

```rust
fn handle_connect_matched(&mut self) {
    if let PinVerifyState::Matched { device_name, addr } = &self.idle_view.pin_verify_state {
        let addr = *addr;
        let name = device_name.clone();
        let pin = self.idle_view.pin_input.clone();
        self.idle_view.connecting = true;
        self.idle_view.connecting_device = Some(name.clone());
        self.idle_view.error = None;
        self.state = AppState::Connecting {
            device_name: name,
            started_at: Instant::now(),
        };
        let _ = self.cmd_tx.send(UiCommand::Connect { addr, pin });
    }
}
```

- [ ] **Step 5: Handle new BackendEvents in `process_backend_events`**

In the `match event` block inside `process_backend_events`, add:

```rust
BackendEvent::PinMatched { device_name, addr } => {
    self.idle_view.pin_verify_state = PinVerifyState::Matched { device_name, addr };
}
BackendEvent::PinNotFound => {
    self.idle_view.pin_verify_state = PinVerifyState::NotFound;
}
```

- [ ] **Step 6: Update `render_ui` to handle `ConnectMatched` and remove `ConnectAny`**

In the `render_ui` method, replace the `IdleAction::ConnectAny` arm with:

```rust
IdleAction::ConnectMatched => {
    self.handle_connect_matched();
}
```

Remove the `ConnectAny` arm entirely.

- [ ] **Step 7: Wire `check_pin_verify` into the `logic()` method**

In `impl eframe::App for App`, inside the `logic` method, add after `self.core.check_connecting_timeout();`:

```rust
self.core.check_pin_verify();
```

- [ ] **Step 8: Run all tests**

Run: `cargo test`
Expected: all tests PASS

- [ ] **Step 9: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: PinVerifyState debounce logic and ConnectMatched handling"
```

---

### Task 6: Backend handles `VerifyPin` — HTTP POST to receivers

**Files:**
- Modify: `src/ui/backend.rs`

- [ ] **Step 1: Add `handle_verify_pin` function**

Add this function to `src/ui/backend.rs`:

```rust
use crate::discovery::browser::DiscoveredReceiver;

fn handle_verify_pin(
    pin: &str,
    devices: &[DiscoveredReceiver],
    event_tx: &Sender<BackendEvent>,
) {
    for device in devices {
        let url = format!("http://{}:{}/verify-pin", device.addr.ip(), device.addr.port());
        let body = serde_json::json!({ "pin": pin });
        match ureq::post(&url)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(2))
            .send_json(&body)
        {
            Ok(response) if response.status() == 200 => {
                if let Ok(json) = response.body_mut().read_json::<serde_json::Value>() {
                    let device_name = json["device_name"]
                        .as_str()
                        .unwrap_or(&device.name)
                        .to_string();
                    let _ = event_tx.send(BackendEvent::PinMatched {
                        device_name,
                        addr: device.addr,
                    });
                    return;
                }
            }
            _ => continue,
        }
    }
    let _ = event_tx.send(BackendEvent::PinNotFound);
}
```

- [ ] **Step 2: Handle `VerifyPin` in `spawn_command_handler`**

In `spawn_command_handler`, inside the Phase 1 loop (waiting for Connect), change the match:

```rust
let (addr, pin) = loop {
    match cmd_rx.recv() {
        Ok(UiCommand::Connect { addr, pin }) => break (addr, pin),
        Ok(UiCommand::VerifyPin { pin }) => {
            // Grab current device list from the mDNS browser's latest snapshot
            // We need to share the device list — see Step 3
            handle_verify_pin(&pin, &current_devices, &event_tx);
        }
        Ok(_) => {}
        Err(_) => return,
    }
};
```

- [ ] **Step 3: Share device list between mDNS browser and command handler**

The mDNS browser thread discovers devices and sends `DevicesUpdated` events. The command handler also needs the current device list for `VerifyPin`. Add a shared `Arc<Mutex<Vec<DiscoveredReceiver>>>`:

In `src/ui/backend.rs`, modify `spawn_mdns_browser` to accept and update the shared list:

```rust
pub fn spawn_mdns_browser(
    event_tx: Sender<BackendEvent>,
    shared_devices: Arc<Mutex<Vec<DiscoveredReceiver>>>,
) -> JoinHandle<()> {
    thread::spawn(move || loop {
        match browser::browse(Duration::from_secs(3)) {
            Ok(devices) => {
                let mut seen = std::collections::HashSet::new();
                let deduped: Vec<_> = devices
                    .into_iter()
                    .filter(|d| seen.insert(d.name.clone()))
                    .collect();
                *shared_devices.lock().unwrap() = deduped.clone();
                if event_tx.send(BackendEvent::DevicesUpdated(deduped)).is_err() {
                    break;
                }
            }
            Err(e) => {
                tracing::warn!("mDNS browse error: {e}");
            }
        }
        thread::sleep(Duration::from_secs(2));
    })
}
```

Modify `spawn_command_handler` to accept the shared list:

```rust
pub fn spawn_command_handler(
    cmd_rx: Receiver<UiCommand>,
    event_tx: Sender<BackendEvent>,
    shared_devices: Arc<Mutex<Vec<DiscoveredReceiver>>>,
) -> JoinHandle<()> {
```

Inside the Phase 1 loop, read the device list when handling `VerifyPin`:

```rust
Ok(UiCommand::VerifyPin { pin }) => {
    let devices = shared_devices.lock().unwrap().clone();
    handle_verify_pin(&pin, &devices, &event_tx);
}
```

Add the required import at the top of `backend.rs`:

```rust
use std::sync::Mutex;
```

- [ ] **Step 4: Update caller in `src/bin/sender.rs`**

Read the current `src/bin/sender.rs` to see how `spawn_mdns_browser` and `spawn_command_handler` are called, and add the shared `Arc<Mutex<Vec<DiscoveredReceiver>>>`:

```rust
let shared_devices = Arc::new(Mutex::new(Vec::new()));
let _browser = backend::spawn_mdns_browser(event_tx.clone(), shared_devices.clone());
let _handler = backend::spawn_command_handler(cmd_rx, event_tx, shared_devices);
```

Add the required imports:

```rust
use std::sync::{Arc, Mutex};
use screen_mirror::discovery::browser::DiscoveredReceiver;
```

- [ ] **Step 5: Run to verify compilation**

Run: `cargo check`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add src/ui/backend.rs src/bin/sender.rs Cargo.toml Cargo.lock
git commit -m "feat: backend handles VerifyPin via HTTP POST to receivers"
```

---

### Task 7: Update debug hotkeys for PinVerifyState testing

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add Cmd+7 debug hotkey for simulating PinMatched**

In the `handle_debug_keys` method in `src/ui/app.rs`, add a new match arm:

```rust
egui::Key::Num7 => {
    self.idle_view.pin_verify_state = PinVerifyState::Matched {
        device_name: "测试电视".to_string(),
        addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 9000),
    };
}
egui::Key::Num8 => {
    self.idle_view.pin_verify_state = PinVerifyState::NotFound;
}
egui::Key::Num9 => {
    self.idle_view.pin_verify_state = PinVerifyState::Verifying;
}
```

- [ ] **Step 2: Run to verify compilation**

Run: `cargo build --bin sender`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: debug hotkeys Cmd+7/8/9 for PinVerifyState"
```
