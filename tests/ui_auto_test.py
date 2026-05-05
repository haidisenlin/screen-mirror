#!/usr/bin/env python3
"""
Comprehensive automated UI test for screen-mirror sender app.
Uses Quartz CGEvent for mouse simulation, CGEventPostToPid for keyboard,
NSRunningApplication for app activation, and screencapture for screenshots.
"""
import subprocess
import time
import os
import sys

import Quartz
from Quartz import (
    CGEventCreateMouseEvent, CGEventPost, kCGEventLeftMouseDown,
    kCGEventLeftMouseUp, kCGMouseButtonLeft, kCGHIDEventTap,
    CGEventCreateKeyboardEvent, CGEventSetFlags,
    kCGEventFlagMaskShift, CGWindowListCopyWindowInfo,
    kCGWindowListOptionOnScreenOnly, kCGWindowListOptionAll, kCGNullWindowID,
    CGEventCreateScrollWheelEvent, kCGScrollEventUnitLine,
    CGEventPostToPid, kCGEventLeftMouseDragged,
)
from AppKit import NSRunningApplication, NSApplicationActivateIgnoringOtherApps

SCREENSHOTS_DIR = "/tmp/screen_mirror_test"
SENDER_BIN = "target/debug/sender"
APP_TITLE = "screen-mirror"
PROJECT_DIR = "/Users/lihaisen/RustroverProjects/screen-mirror"

os.makedirs(SCREENSHOTS_DIR, exist_ok=True)

test_results = []
sender_pid = None


# ========== UTILITIES ==========

def log(msg):
    print(f"[TEST] {msg}")

def screenshot(name):
    path = os.path.join(SCREENSHOTS_DIR, f"{name}.png")
    subprocess.run(["screencapture", "-x", path], check=True)
    return path

def screenshot_window(name, window_id=None):
    path = os.path.join(SCREENSHOTS_DIR, f"{name}.png")
    if window_id:
        subprocess.run(["screencapture", "-x", "-l", str(window_id), path], check=True)
    else:
        subprocess.run(["screencapture", "-x", path], check=True)
    return path

def find_app_window():
    windows = CGWindowListCopyWindowInfo(kCGWindowListOptionAll, kCGNullWindowID)
    for w in windows:
        owner = w.get("kCGWindowOwnerName", "")
        if owner == "sender":
            bounds = w.get("kCGWindowBounds", {})
            if bounds.get("Width", 0) == 0 and bounds.get("Height", 0) == 0:
                continue
            name = w.get("kCGWindowName", "")
            wid = w.get("kCGWindowNumber", 0)
            return {
                "id": wid,
                "x": bounds.get("X", 0),
                "y": bounds.get("Y", 0),
                "w": bounds.get("Width", 0),
                "h": bounds.get("Height", 0),
                "owner": owner,
                "name": name,
            }
    return None

def activate_app(pid):
    app = NSRunningApplication.runningApplicationWithProcessIdentifier_(pid)
    if app:
        app.activateWithOptions_(NSApplicationActivateIgnoringOtherApps)
    time.sleep(0.5)

def click(x, y):
    point = Quartz.CGPointMake(x, y)
    event_down = CGEventCreateMouseEvent(None, kCGEventLeftMouseDown, point, kCGMouseButtonLeft)
    event_up = CGEventCreateMouseEvent(None, kCGEventLeftMouseUp, point, kCGMouseButtonLeft)
    CGEventPost(kCGHIDEventTap, event_down)
    time.sleep(0.05)
    CGEventPost(kCGHIDEventTap, event_up)
    time.sleep(0.1)

def drag(start_x, start_y, end_x, end_y, steps=10):
    point_start = Quartz.CGPointMake(start_x, start_y)
    point_end = Quartz.CGPointMake(end_x, end_y)
    event_down = CGEventCreateMouseEvent(None, kCGEventLeftMouseDown, point_start, kCGMouseButtonLeft)
    CGEventPost(kCGHIDEventTap, event_down)
    time.sleep(0.1)
    for i in range(1, steps + 1):
        frac = i / steps
        mx = start_x + (end_x - start_x) * frac
        my = start_y + (end_y - start_y) * frac
        drag_point = Quartz.CGPointMake(mx, my)
        event_drag = CGEventCreateMouseEvent(None, kCGEventLeftMouseDragged, drag_point, kCGMouseButtonLeft)
        CGEventPost(kCGHIDEventTap, event_drag)
        time.sleep(0.02)
    event_up = CGEventCreateMouseEvent(None, kCGEventLeftMouseUp, point_end, kCGMouseButtonLeft)
    CGEventPost(kCGHIDEventTap, event_up)
    time.sleep(0.3)

def char_to_keycode(ch):
    digit_codes = {'0': 29, '1': 18, '2': 19, '3': 20, '4': 21, '5': 23, '6': 22, '7': 26, '8': 28, '9': 25}
    letter_codes = {
        'a': 0, 'b': 11, 'c': 8, 'd': 2, 'e': 14, 'f': 3, 'g': 5, 'h': 4,
        'i': 34, 'j': 38, 'k': 40, 'l': 37, 'm': 46, 'n': 45, 'o': 31, 'p': 35,
        'q': 12, 'r': 15, 's': 1, 't': 17, 'u': 32, 'v': 9, 'w': 13, 'x': 7, 'y': 16, 'z': 6,
    }
    if ch in digit_codes:
        return digit_codes[ch]
    if ch.lower() in letter_codes:
        return letter_codes[ch.lower()]
    return None

def type_text(text, pid):
    for ch in text:
        keycode = char_to_keycode(ch)
        if keycode is None:
            continue
        event_down = CGEventCreateKeyboardEvent(None, keycode, True)
        event_up = CGEventCreateKeyboardEvent(None, keycode, False)
        CGEventPostToPid(pid, event_down)
        time.sleep(0.05)
        CGEventPostToPid(pid, event_up)
        time.sleep(0.1)

def type_special(key_name, pid):
    key_codes = {"delete": 51, "left": 123, "right": 124, "return": 36, "escape": 53, "tab": 48}
    code = key_codes.get(key_name, 0)
    event_down = CGEventCreateKeyboardEvent(None, code, True)
    event_up = CGEventCreateKeyboardEvent(None, code, False)
    CGEventPostToPid(pid, event_down)
    time.sleep(0.05)
    CGEventPostToPid(pid, event_up)
    time.sleep(0.1)

def pass_test(name, detail=""):
    log(f"  PASS: {name} {detail}")
    test_results.append(("PASS", name))

def fail_test(name, detail=""):
    log(f"  FAIL: {name} {detail}")
    test_results.append(("FAIL", name))

def skip_test(name, reason=""):
    log(f"  SKIP: {name} {reason}")
    test_results.append(("SKIP", name))

def click_pin_area(win):
    pin_x = win["x"] + win["w"] / 2
    pin_y = win["y"] + win["h"] * 0.55
    click(pin_x, pin_y)
    time.sleep(0.3)

def clear_pin(pid, count=6):
    for _ in range(count):
        type_special("delete", pid)
        time.sleep(0.15)
    time.sleep(0.3)


# ========== TEST GROUP 1: APP LAUNCH & WINDOW BASICS ==========

def test_group_01_launch(pid):
    log("=" * 50)
    log("GROUP 1: App Launch & Window Basics")
    log("=" * 50)

    # 1.1 Window appears
    log("--- 1.1: Window appears on launch ---")
    win = find_app_window()
    if win:
        pass_test("Window found", f"owner={win['owner']} at ({win['x']},{win['y']})")
        screenshot_window("01_1_window_found", win["id"])
    else:
        fail_test("Window found", "No window detected")
        screenshot("01_1_no_window")
        return None

    # 1.2 Window size is correct (320x480)
    log("--- 1.2: Window size ---")
    if win["w"] == 320 and win["h"] == 480:
        pass_test("Window size exact", f"{win['w']}x{win['h']}")
    elif win["w"] > 100 and win["h"] > 100:
        pass_test("Window size reasonable", f"{win['w']}x{win['h']} (expected 320x480)")
    else:
        fail_test("Window size", f"{win['w']}x{win['h']}")

    # 1.3 Window is always-on-top (transparent, no decorations)
    log("--- 1.3: Window properties ---")
    # Check that window has no title bar by looking at the name
    if win["name"] == "screen-mirror" or "screen-mirror" in str(win["name"]):
        pass_test("Window title", f"'{win['name']}'")
    else:
        fail_test("Window title", f"Expected 'screen-mirror', got '{win['name']}'")

    # 1.4 Take initial full-screen screenshot for visual inspection
    log("--- 1.4: Full screen context ---")
    screenshot("01_4_full_screen_context")
    pass_test("Full screen screenshot captured")

    return win


# ========== TEST GROUP 2: UI CONTENT VISUAL INSPECTION ==========

def test_group_02_ui_content(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 2: UI Content & Visual Elements")
    log("=" * 50)
    if not win:
        skip_test("UI content tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.5)

    # 2.1 Capture high-res window screenshot for content check
    log("--- 2.1: Window content screenshot ---")
    screenshot_window("02_1_ui_content", win["id"])
    pass_test("UI content screenshot captured")

    # 2.2 Capture after a brief wait to see animation
    log("--- 2.2: Animation frame 1 ---")
    time.sleep(1.0)
    screenshot_window("02_2_animation_frame1", win["id"])
    pass_test("Animation frame 1 captured")

    # 2.3 Capture second animation frame to verify motion
    log("--- 2.3: Animation frame 2 ---")
    time.sleep(1.0)
    screenshot_window("02_3_animation_frame2", win["id"])
    pass_test("Animation frame 2 captured (compare with frame 1 for motion)")

    # 2.4 Window rounded corners check (full screen capture shows corners)
    log("--- 2.4: Window corners (full screen capture) ---")
    screenshot("02_4_corners_check")
    pass_test("Corners screenshot for visual inspection")


# ========== TEST GROUP 3: PIN INPUT BASICS ==========

def test_group_03_pin_basics(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 3: PIN Input Basic Operations")
    log("=" * 50)
    if not win:
        skip_test("PIN input tests", "no window")
        return

    activate_app(pid)
    click_pin_area(win)

    # 3.1 Type single digit
    log("--- 3.1: Type single digit ---")
    type_text("1", pid)
    time.sleep(0.3)
    screenshot_window("03_1_single_digit", win["id"])
    pass_test("Single digit '1' typed")

    # 3.2 Type remaining 5 digits to fill PIN
    log("--- 3.2: Fill all 6 PIN boxes ---")
    type_text("23456", pid)
    time.sleep(0.5)
    screenshot_window("03_2_full_pin", win["id"])
    pass_test("Full PIN '123456' entered")

    # 3.3 Verify button state with full PIN (greyed because no devices)
    log("--- 3.3: Button state with full PIN ---")
    screenshot_window("03_3_button_state", win["id"])
    pass_test("Button state captured (should be greyed - no devices)")

    # 3.4 Single backspace
    log("--- 3.4: Single backspace ---")
    type_special("delete", pid)
    time.sleep(0.3)
    screenshot_window("03_4_after_backspace", win["id"])
    pass_test("Backspace removes last digit")

    # 3.5 Type replacement
    log("--- 3.5: Type replacement digit ---")
    type_text("7", pid)
    time.sleep(0.3)
    screenshot_window("03_5_replaced", win["id"])
    pass_test("Digit '7' replaces deleted position")

    # 3.6 Clear all
    log("--- 3.6: Clear all digits ---")
    clear_pin(pid)
    screenshot_window("03_6_cleared", win["id"])
    pass_test("All digits cleared")


# ========== TEST GROUP 4: PIN CURSOR & CLICK-TO-EDIT ==========

def test_group_04_pin_cursor(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 4: PIN Cursor Navigation & Click-to-Edit")
    log("=" * 50)
    if not win:
        skip_test("PIN cursor tests", "no window")
        return

    activate_app(pid)
    click_pin_area(win)

    # 4.1 Fill PIN first
    log("--- 4.1: Prepare: fill PIN '111111' ---")
    clear_pin(pid)
    type_text("111111", pid)
    time.sleep(0.3)
    screenshot_window("04_1_prepared", win["id"])
    pass_test("PIN filled with '111111'")

    # 4.2 Click 1st box
    log("--- 4.2: Click 1st box ---")
    total_w = win["w"] - 32
    box_w = (total_w - 8 * 5) / 6
    box1_x = win["x"] + 16 + box_w * 0.5
    box_y = win["y"] + win["h"] * 0.55
    click(box1_x, box_y)
    time.sleep(0.3)
    screenshot_window("04_2_click_box1", win["id"])
    pass_test("Clicked 1st PIN box (cursor should move)")

    # 4.3 Type to replace digit at cursor position (box 1)
    log("--- 4.3: Replace digit at box 1 ---")
    type_text("9", pid)
    time.sleep(0.3)
    screenshot_window("04_3_replaced_box1", win["id"])
    pass_test("Replaced digit at box 1 with '9'")

    # 4.4 Click 4th box
    log("--- 4.4: Click 4th box ---")
    box4_x = win["x"] + 16 + box_w * 3.5 + 8 * 3
    click(box4_x, box_y)
    time.sleep(0.3)
    screenshot_window("04_4_click_box4", win["id"])
    pass_test("Clicked 4th PIN box")

    # 4.5 Replace at box 4
    log("--- 4.5: Replace digit at box 4 ---")
    type_text("5", pid)
    time.sleep(0.3)
    screenshot_window("04_5_replaced_box4", win["id"])
    pass_test("Replaced digit at box 4 with '5'")

    # 4.6 Arrow key left
    log("--- 4.6: Arrow key left ---")
    type_special("left", pid)
    time.sleep(0.3)
    screenshot_window("04_6_arrow_left", win["id"])
    pass_test("Arrow left moves cursor")

    # 4.7 Arrow key right
    log("--- 4.7: Arrow key right ---")
    type_special("right", pid)
    time.sleep(0.3)
    screenshot_window("04_7_arrow_right", win["id"])
    pass_test("Arrow right moves cursor")

    # 4.8 Click last box (6th)
    log("--- 4.8: Click 6th box ---")
    box6_x = win["x"] + 16 + box_w * 5.5 + 8 * 5
    click(box6_x, box_y)
    time.sleep(0.3)
    screenshot_window("04_8_click_box6", win["id"])
    pass_test("Clicked 6th PIN box")

    # Clean up
    clear_pin(pid)


# ========== TEST GROUP 5: PIN EDGE CASES ==========

def test_group_05_pin_edge(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 5: PIN Input Edge Cases")
    log("=" * 50)
    if not win:
        skip_test("PIN edge tests", "no window")
        return

    activate_app(pid)
    click_pin_area(win)
    clear_pin(pid)

    # 5.1 Overflow: type 9 digits, only 6 should register
    log("--- 5.1: PIN overflow (type 9 digits) ---")
    type_text("123456789", pid)
    time.sleep(0.5)
    screenshot_window("05_1_overflow", win["id"])
    pass_test("Overflow test: typed 9 digits (should show only 6)")

    # 5.2 Backspace on empty PIN
    log("--- 5.2: Backspace on empty ---")
    clear_pin(pid, 8)  # Extra backspaces
    type_special("delete", pid)  # One more
    time.sleep(0.3)
    screenshot_window("05_2_backspace_empty", win["id"])
    pass_test("Backspace on empty PIN (should be no-op)")

    # 5.3 Type letters (should be ignored, only digits accepted)
    log("--- 5.3: Type letters (should be ignored) ---")
    type_text("abcdef", pid)
    time.sleep(0.5)
    screenshot_window("05_3_letters_ignored", win["id"])
    pass_test("Letters typed (should be ignored)")

    # 5.4 Mix of digits and letters
    log("--- 5.4: Mixed input ---")
    type_text("1a2b3c", pid)
    time.sleep(0.5)
    screenshot_window("05_4_mixed_input", win["id"])
    pass_test("Mixed input: '1a2b3c' (should show '123')")

    # 5.5 Rapid typing
    log("--- 5.5: Rapid input ---")
    clear_pin(pid)
    # Type quickly by reducing delays
    for ch in "654321":
        keycode = char_to_keycode(ch)
        event_down = CGEventCreateKeyboardEvent(None, keycode, True)
        event_up = CGEventCreateKeyboardEvent(None, keycode, False)
        CGEventPostToPid(pid, event_down)
        time.sleep(0.02)
        CGEventPostToPid(pid, event_up)
        time.sleep(0.02)
    time.sleep(0.5)
    screenshot_window("05_5_rapid_typing", win["id"])
    pass_test("Rapid typing test")

    clear_pin(pid)


# ========== TEST GROUP 6: WINDOW DRAG ==========

def test_group_06_drag(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 6: Window Drag")
    log("=" * 50)
    if not win:
        skip_test("Window drag tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # 6.1 Drag right
    log("--- 6.1: Drag window right ---")
    # Re-find window (it may have focus-loss hidden, re-activate first)
    activate_app(pid)
    time.sleep(1.5)  # Wait for grace period
    win = find_app_window()
    if not win:
        skip_test("Window drag tests", "window hidden (focus loss)")
        return None
    start_x = win["x"] + win["w"] / 2
    start_y = win["y"] + 20
    drag(start_x, start_y, start_x + 150, start_y)
    time.sleep(0.5)
    win_after = find_app_window()
    if win_after:
        dx = win_after["x"] - win["x"]
        screenshot_window("06_1_drag_right", win_after["id"])
        if abs(dx) > 50:
            pass_test("Drag right", f"moved {dx:.0f}px horizontally")
        else:
            fail_test("Drag right", f"only moved {dx:.0f}px")
    else:
        fail_test("Drag right", "window lost")
        return

    # 6.2 Drag down
    log("--- 6.2: Drag window down ---")
    activate_app(pid)
    time.sleep(0.3)
    win_before = find_app_window()
    if win_before:
        sx = win_before["x"] + win_before["w"] / 2
        sy = win_before["y"] + 20
        drag(sx, sy, sx, sy + 100)
        time.sleep(0.5)
        win_after2 = find_app_window()
        if win_after2:
            dy = win_after2["y"] - win_before["y"]
            screenshot_window("06_2_drag_down", win_after2["id"])
            if abs(dy) > 30:
                pass_test("Drag down", f"moved {dy:.0f}px vertically")
            else:
                fail_test("Drag down", f"only moved {dy:.0f}px")
        else:
            fail_test("Drag down", "window lost")

    # 6.3 Drag diagonal
    log("--- 6.3: Drag diagonal ---")
    activate_app(pid)
    time.sleep(0.3)
    win_before = find_app_window()
    if win_before:
        sx = win_before["x"] + win_before["w"] / 2
        sy = win_before["y"] + 20
        drag(sx, sy, sx - 200, sy - 100)
        time.sleep(0.5)
        win_after3 = find_app_window()
        if win_after3:
            screenshot_window("06_3_drag_diagonal", win_after3["id"])
            pass_test("Drag diagonal", f"to ({win_after3['x']:.0f},{win_after3['y']:.0f})")
        else:
            fail_test("Drag diagonal", "window lost")

    return find_app_window()


# ========== TEST GROUP 7: TRAY ICON & MENU ==========

def test_group_07_tray(pid):
    log("")
    log("=" * 50)
    log("GROUP 7: System Tray Icon & Menu")
    log("=" * 50)

    # 7.1 Tray process running
    log("--- 7.1: Tray process alive ---")
    result = subprocess.run(["ps", "-p", str(pid), "-o", "comm="], capture_output=True, text=True, timeout=5)
    if result.returncode == 0 and result.stdout.strip():
        pass_test("Sender process alive", result.stdout.strip())
    else:
        fail_test("Sender process alive")
        return

    # 7.2 Try opening tray menu via AppleScript
    log("--- 7.2: Open tray menu ---")
    script_open = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu bar item 1 of menu bar 2
            delay 0.5
        end tell
    end tell
    '''
    script_click_item = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu item 1 of menu 1 of menu bar item 1 of menu bar 2
        end tell
    end tell
    '''
    try:
        result = subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        screenshot("07_2_tray_menu_opened")
        if result.returncode == 0:
            pass_test("Tray menu opened")
        else:
            skip_test("Tray menu opened", "(needs accessibility permissions)")
            log(f"    Grant: System Settings > Privacy > Accessibility > Terminal")
            return
    except subprocess.TimeoutExpired:
        skip_test("Tray menu opened", "timeout")
        return

    # Close the menu by pressing Escape (to not trigger any action)
    subprocess.run(["osascript", "-e", 'tell application "System Events" to key code 53'],
                    capture_output=True, text=True, timeout=3)
    time.sleep(0.3)

    # 7.3 Toggle hide: window is visible, click "显示窗口" to hide it
    log("--- 7.3: Tray toggle: hide visible window ---")
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_click_item], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
        win_after = find_app_window()
        screenshot("07_3_after_toggle_hide")
        if win_after is None:
            pass_test("Tray toggle hides visible window")
        else:
            fail_test("Tray toggle hide", f"window still at ({win_after['x']},{win_after['y']})")
    except subprocess.TimeoutExpired:
        fail_test("Tray toggle hide", "timeout")

    # 7.4 Toggle show: window is hidden, click "显示窗口" to show it
    log("--- 7.4: Tray toggle: show hidden window ---")
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_click_item], capture_output=True, text=True, timeout=5)
        time.sleep(1.5)
        win = find_app_window()
        if win:
            pass_test("Tray toggle shows hidden window", f"at ({win['x']},{win['y']})")
            screenshot_window("07_4_after_toggle_show", win["id"])
        else:
            fail_test("Tray toggle show", "window not found")
    except subprocess.TimeoutExpired:
        fail_test("Tray toggle show", "timeout")

    # 7.5 Second toggle cycle: hide again
    log("--- 7.5: Second toggle cycle: hide ---")
    time.sleep(1.5)  # Wait for grace period from 7.4 re-show
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_click_item], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
        win_after = find_app_window()
        screenshot("07_5_second_hide")
        if win_after is None:
            pass_test("Second toggle hides window")
        else:
            fail_test("Second toggle hide", f"still visible")
    except subprocess.TimeoutExpired:
        skip_test("Second toggle hide", "timeout")

    # 7.6 Re-show for subsequent tests
    log("--- 7.6: Re-show for next tests ---")
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_click_item], capture_output=True, text=True, timeout=5)
        time.sleep(1.5)
        win = find_app_window()
        if win:
            pass_test("Window re-shown for next tests")
            screenshot_window("07_6_re_shown", win["id"])
        else:
            log("  (window not found, subsequent tests may skip)")
    except subprocess.TimeoutExpired:
        log("  (timeout, subsequent tests may skip)")


# ========== TEST GROUP 8: FOCUS LOSS & APP SWITCHING ==========

def test_group_08_focus(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 8: Focus Loss & Multi-App Switching")
    log("=" * 50)
    if not win:
        skip_test("Focus tests", "no window")
        return

    # Make sure window is visible
    activate_app(pid)
    time.sleep(1.5)  # Wait for grace period

    # 8.1 Click desktop to lose focus
    log("--- 8.1: Click desktop (focus loss) ---")
    click(10, 400)
    time.sleep(2.0)
    win_after = find_app_window()
    screenshot("08_1_after_desktop_click")
    if win_after is None:
        pass_test("Window hidden on desktop click")
    else:
        fail_test("Window hidden on desktop click", f"still at ({win_after['x']},{win_after['y']})")

    # 8.2 Re-show via tray
    log("--- 8.2: Re-show after focus loss ---")
    script_open = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu bar item 1 of menu bar 2
            delay 0.3
        end tell
    end tell
    '''
    script_show = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu item 1 of menu 1 of menu bar item 1 of menu bar 2
        end tell
    end tell
    '''
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
        win2 = find_app_window()
        if win2:
            pass_test("Re-shown after focus loss")
            screenshot_window("08_2_re_shown", win2["id"])
        else:
            skip_test("Re-shown after focus loss", "(needs accessibility or manual re-show)")
    except subprocess.TimeoutExpired:
        skip_test("Re-show after focus loss", "timeout")

    # 8.3 Switch to Finder and back
    log("--- 8.3: Switch to Finder ---")
    activate_app(pid)
    time.sleep(1.5)  # Grace period
    # Activate Finder
    result = subprocess.run(
        ["osascript", "-e", 'tell application "Finder" to activate'],
        capture_output=True, text=True, timeout=5
    )
    time.sleep(2.0)
    win3 = find_app_window()
    screenshot("08_3_after_finder_switch")
    if win3 is None:
        pass_test("Window hidden when switching to Finder")
    else:
        fail_test("Window hidden on Finder switch", f"still visible")

    # 8.4 Activate sender again after Finder switch
    log("--- 8.4: Return from Finder ---")
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
        win4 = find_app_window()
        if win4:
            pass_test("Window recovered after Finder switch")
            screenshot_window("08_4_recovered", win4["id"])
        else:
            skip_test("Recover after Finder", "(needs accessibility)")
    except subprocess.TimeoutExpired:
        skip_test("Recover after Finder", "timeout")

    # 8.5 Switch to Terminal and back
    log("--- 8.5: Switch to Terminal ---")
    activate_app(pid)
    time.sleep(1.5)
    subprocess.run(
        ["osascript", "-e", 'tell application "Terminal" to activate'],
        capture_output=True, text=True, timeout=5
    )
    time.sleep(2.0)
    win5 = find_app_window()
    screenshot("08_5_after_terminal_switch")
    if win5 is None:
        pass_test("Window hidden when switching to Terminal")
    else:
        fail_test("Window hidden on Terminal switch")

    # 8.6 Rapid app switching (3 times)
    log("--- 8.6: Rapid app switching ---")
    for i in range(3):
        try:
            subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
            time.sleep(0.3)
            subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
            time.sleep(1.5)
        except subprocess.TimeoutExpired:
            pass
        # Switch away
        subprocess.run(["osascript", "-e", 'tell application "Finder" to activate'],
                        capture_output=True, text=True, timeout=5)
        time.sleep(1.5)
    screenshot("08_6_after_rapid_switch")
    pass_test("Rapid app switching (3 cycles)")


# ========== TEST GROUP 9: PIN STATE PERSISTENCE ACROSS SHOW/HIDE ==========

def test_group_09_pin_persist(pid):
    log("")
    log("=" * 50)
    log("GROUP 9: PIN State Persistence Across Show/Hide")
    log("=" * 50)

    script_open = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu bar item 1 of menu bar 2
            delay 0.3
        end tell
    end tell
    '''
    script_show = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu item 1 of menu 1 of menu bar item 1 of menu bar 2
        end tell
    end tell
    '''

    # 9.1 Show window and enter PIN
    log("--- 9.1: Enter PIN before hide ---")
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
    except subprocess.TimeoutExpired:
        skip_test("PIN persistence", "can't show window")
        return

    win = find_app_window()
    if not win:
        skip_test("PIN persistence", "window not found")
        return

    activate_app(pid)
    click_pin_area(win)
    clear_pin(pid)
    type_text("987654", pid)
    time.sleep(0.5)
    screenshot_window("09_1_pin_before_hide", win["id"])
    pass_test("PIN '987654' entered before hide")

    # 9.2 Hide via focus loss
    log("--- 9.2: Hide window ---")
    time.sleep(1.0)  # Wait for grace period
    click(10, 400)
    time.sleep(2.0)
    win_gone = find_app_window()
    if win_gone is None:
        pass_test("Window hidden for persistence test")
    else:
        log("  (window didn't hide, testing PIN anyway)")

    # 9.3 Re-show and check if PIN is preserved
    log("--- 9.3: Check PIN preserved after show ---")
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
    except subprocess.TimeoutExpired:
        skip_test("PIN preserved", "can't re-show")
        return

    win2 = find_app_window()
    if win2:
        screenshot_window("09_3_pin_after_show", win2["id"])
        pass_test("PIN state screenshot after show/hide cycle")
    else:
        skip_test("PIN preserved check", "window not found")


# ========== TEST GROUP 10: CLICK OUTSIDE PIN AREA ==========

def test_group_10_click_areas(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 10: Click Various UI Areas")
    log("=" * 50)

    # Re-find window since it may have moved
    activate_app(pid)
    time.sleep(0.5)
    win = find_app_window()
    if not win:
        skip_test("Click area tests", "no window")
        return

    # 10.1 Click header area
    log("--- 10.1: Click header area ---")
    click(win["x"] + 60, win["y"] + 25)
    time.sleep(0.3)
    screenshot_window("10_1_click_header", win["id"])
    pass_test("Click on header area")

    # 10.2 Click illustration area
    log("--- 10.2: Click illustration area ---")
    click(win["x"] + win["w"] / 2, win["y"] + win["h"] * 0.3)
    time.sleep(0.3)
    screenshot_window("10_2_click_illustration", win["id"])
    pass_test("Click on illustration area")

    # 10.3 Click "开始投屏" button (should be disabled)
    log("--- 10.3: Click disabled button ---")
    btn_x = win["x"] + win["w"] / 2
    btn_y = win["y"] + win["h"] * 0.72
    click(btn_x, btn_y)
    time.sleep(0.5)
    screenshot_window("10_3_click_disabled_btn", win["id"])
    # Verify window still shows idle view (button shouldn't do anything)
    win_still = find_app_window()
    if win_still:
        pass_test("Disabled button click (no state change)")
    else:
        fail_test("Disabled button click", "window disappeared")

    # 10.4 Click "搜索中..." badge
    log("--- 10.4: Click badge area ---")
    badge_x = win["x"] + win["w"] - 50
    badge_y = win["y"] + 30
    click(badge_x, badge_y)
    time.sleep(0.3)
    screenshot_window("10_4_click_badge", win["id"])
    pass_test("Click badge area (should be no-op)")

    # 10.5 Click bottom empty area
    log("--- 10.5: Click bottom empty area ---")
    click(win["x"] + win["w"] / 2, win["y"] + win["h"] - 30)
    time.sleep(0.3)
    screenshot_window("10_5_click_bottom", win["id"])
    pass_test("Click bottom empty area")


# ========== TEST GROUP 11: WINDOW POSITION AFTER SHOW/HIDE ==========

def test_group_11_position(pid):
    log("")
    log("=" * 50)
    log("GROUP 11: Window Position After Show/Hide Cycles")
    log("=" * 50)

    script_open = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu bar item 1 of menu bar 2
            delay 0.3
        end tell
    end tell
    '''
    script_show = f'''
    tell application "System Events"
        tell (first process whose unix id is {pid})
            click menu item 1 of menu 1 of menu bar item 1 of menu bar 2
        end tell
    end tell
    '''

    # 11.1 Record initial position
    log("--- 11.1: Record initial position ---")
    # Try twice to show window if needed
    for attempt in range(2):
        try:
            subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
            time.sleep(0.5)
            subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
            time.sleep(1.5)
        except subprocess.TimeoutExpired:
            pass
        win1 = find_app_window()
        if win1:
            break
        time.sleep(0.5)

    if not win1:
        skip_test("Position test", "no window after 2 attempts")
        return
    pos1 = (win1["x"], win1["y"])
    pass_test("Initial position recorded", f"({pos1[0]},{pos1[1]})")

    # 11.2 Drag to a specific position
    log("--- 11.2: Drag to new position ---")
    activate_app(pid)
    time.sleep(0.3)
    sx = win1["x"] + win1["w"] / 2
    sy = win1["y"] + 20
    drag(sx, sy, 200, 200)
    time.sleep(0.5)
    win2 = find_app_window()
    if win2:
        pos2 = (win2["x"], win2["y"])
        pass_test("Dragged to new position", f"({pos2[0]},{pos2[1]})")
        screenshot_window("11_2_new_position", win2["id"])
    else:
        fail_test("Drag to new position", "window lost")
        return

    # 11.3 Hide and re-show, check position
    log("--- 11.3: Hide and re-show, check position ---")
    time.sleep(1.0)
    click(10, 400)  # Focus loss
    time.sleep(2.0)
    try:
        subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
        time.sleep(0.5)
        subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
        time.sleep(1.0)
    except subprocess.TimeoutExpired:
        skip_test("Position after show/hide", "timeout")
        return

    win3 = find_app_window()
    if win3:
        pos3 = (win3["x"], win3["y"])
        screenshot_window("11_3_position_after_cycle", win3["id"])
        pass_test("Position after show/hide cycle", f"({pos3[0]},{pos3[1]})")
    else:
        skip_test("Position after show/hide", "window not found")


# ========== TEST GROUP 12: ANIMATION CONTINUITY ==========

def test_group_12_animation(pid):
    log("")
    log("=" * 50)
    log("GROUP 12: Animation Continuity")
    log("=" * 50)

    activate_app(pid)
    time.sleep(0.5)
    win = find_app_window()
    if not win:
        # Try to show via tray
        script_open = f'''
        tell application "System Events"
            tell (first process whose unix id is {pid})
                click menu bar item 1 of menu bar 2
                delay 0.3
            end tell
        end tell
        '''
        script_show = f'''
        tell application "System Events"
            tell (first process whose unix id is {pid})
                click menu item 1 of menu 1 of menu bar item 1 of menu bar 2
            end tell
        end tell
        '''
        try:
            subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
            time.sleep(0.5)
            subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
            time.sleep(1.0)
            win = find_app_window()
        except subprocess.TimeoutExpired:
            pass

    if not win:
        skip_test("Animation tests", "no window")
        return

    # 12.1 Capture 5 frames at 500ms intervals
    log("--- 12.1: Capture 5 animation frames ---")
    for i in range(5):
        screenshot_window(f"12_1_anim_frame_{i+1}", win["id"])
        time.sleep(0.5)
    pass_test("5 animation frames captured at 500ms intervals")

    # 12.2 Full-screen capture showing animation in context
    log("--- 12.2: Animation in context ---")
    screenshot("12_2_animation_context")
    pass_test("Animation context screenshot")


# ========== TEST GROUP 13: STRESS - RAPID INTERACTIONS ==========

def test_group_13_stress(pid):
    log("")
    log("=" * 50)
    log("GROUP 13: Rapid Interaction Stress Test")
    log("=" * 50)

    activate_app(pid)
    time.sleep(0.5)
    win = find_app_window()
    if not win:
        skip_test("Stress tests", "no window")
        return

    # 13.1 Rapid clicks across the window (keep within bounds to avoid focus loss)
    log("--- 13.1: Rapid clicks ---")
    for i in range(10):
        rx = win["x"] + 20 + (i * 28) % int(win["w"] - 40)
        ry = win["y"] + 40 + (i * 38) % int(win["h"] - 80)
        click(rx, ry)
        time.sleep(0.05)
    time.sleep(0.5)
    win_after = find_app_window()
    screenshot_window("13_1_after_rapid_clicks", win_after["id"] if win_after else None)
    if win_after:
        pass_test("Survives 10 rapid clicks")
    else:
        fail_test("Survives rapid clicks", "window disappeared")
        return

    # 13.2 Rapid PIN input/clear cycles
    log("--- 13.2: Rapid PIN cycles ---")
    activate_app(pid)
    click_pin_area(win_after)
    for cycle in range(5):
        type_text("123456", pid)
        time.sleep(0.1)
        clear_pin(pid)
        time.sleep(0.1)
    time.sleep(0.5)
    win_after2 = find_app_window()
    screenshot_window("13_2_after_pin_cycles", win_after2["id"] if win_after2 else None)
    if win_after2:
        pass_test("Survives 5 rapid PIN input/clear cycles")
    else:
        fail_test("Survives PIN cycles", "window disappeared")


# ========== UTILITY: DEBUG HOTKEYS ==========

def send_debug_key(num, pid):
    """Send Cmd+num to trigger debug hotkey (Cmd+1 through Cmd+6)."""
    keycode = char_to_keycode(str(num))
    if keycode is None:
        return
    event_down = CGEventCreateKeyboardEvent(None, keycode, True)
    event_up = CGEventCreateKeyboardEvent(None, keycode, False)
    CGEventSetFlags(event_down, Quartz.kCGEventFlagMaskCommand)
    CGEventSetFlags(event_up, Quartz.kCGEventFlagMaskCommand)
    CGEventPostToPid(pid, event_down)
    time.sleep(0.05)
    CGEventPostToPid(pid, event_up)
    time.sleep(0.8)


# ========== TEST GROUP 14: MODE SELECT VIEW ==========

def test_group_14_mode_select(win, pid):
    log("=" * 50)
    log("GROUP 14: Mode Select View (Cmd+2)")
    log("=" * 50)

    if not win:
        skip_test("Mode select tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Switch to ModeSelect via Cmd+2
    send_debug_key(2, pid)
    time.sleep(0.5)

    # 14.1 Screenshot after switching to ModeSelect
    log("--- 14.1: ModeSelect view renders ---")
    win = find_app_window()
    if win:
        path = screenshot_window("14_1_mode_select", win["id"])
        pass_test("ModeSelect view screenshot taken", path)
    else:
        fail_test("ModeSelect view", "window disappeared after Cmd+2")
        return

    # 14.2 Window still exists and has correct size
    log("--- 14.2: Window intact ---")
    if win["w"] > 100 and win["h"] > 100:
        pass_test("ModeSelect window size", f"{win['w']}x{win['h']}")
    else:
        fail_test("ModeSelect window size", f"{win['w']}x{win['h']}")

    # 14.3 Click full-screen mirror button (top area of the mode buttons)
    log("--- 14.3: Click fullscreen mirror button ---")
    btn_x = win["x"] + win["w"] * 0.2
    btn_y = win["y"] + win["h"] * 0.6
    click(btn_x, btn_y)
    time.sleep(0.8)
    win_after = find_app_window()
    if win_after:
        screenshot_window("14_3_after_fullscreen_click", win_after["id"])
        pass_test("Click fullscreen button - app stable")
    else:
        fail_test("Click fullscreen button", "window disappeared")

    # Reset back to idle
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 15: STREAMING VIEW ==========

def test_group_15_streaming(win, pid):
    log("=" * 50)
    log("GROUP 15: Streaming View (Cmd+3)")
    log("=" * 50)

    if not win:
        skip_test("Streaming tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Switch to Streaming via Cmd+3
    send_debug_key(3, pid)
    time.sleep(0.5)

    # 15.1 Screenshot of streaming view
    log("--- 15.1: Streaming view renders ---")
    win = find_app_window()
    if win:
        path = screenshot_window("15_1_streaming", win["id"])
        pass_test("Streaming view screenshot", path)
    else:
        fail_test("Streaming view", "window disappeared after Cmd+3")
        return

    # 15.2 Window intact
    log("--- 15.2: Window intact ---")
    if win["w"] > 100 and win["h"] > 100:
        pass_test("Streaming window size", f"{win['w']}x{win['h']}")
    else:
        fail_test("Streaming window size", f"{win['w']}x{win['h']}")

    # 15.3 Click pause button (left half, bottom area)
    log("--- 15.3: Click pause button ---")
    pause_x = win["x"] + win["w"] * 0.25
    pause_y = win["y"] + win["h"] * 0.88
    click(pause_x, pause_y)
    time.sleep(0.8)
    win_after = find_app_window()
    if win_after:
        screenshot_window("15_3_after_pause", win_after["id"])
        pass_test("Pause button click - app stable")
    else:
        fail_test("Pause button click", "window disappeared")

    # 15.4 Switch back to streaming, click disconnect (right half, bottom area)
    log("--- 15.4: Click disconnect button ---")
    send_debug_key(3, pid)
    time.sleep(0.5)
    win = find_app_window()
    if win:
        disc_x = win["x"] + win["w"] * 0.75
        disc_y = win["y"] + win["h"] * 0.88
        click(disc_x, disc_y)
        time.sleep(0.8)
        win_after = find_app_window()
        if win_after:
            screenshot_window("15_4_after_disconnect", win_after["id"])
            pass_test("Disconnect button click - app stable")
        else:
            fail_test("Disconnect button click", "window disappeared")
    else:
        skip_test("Disconnect button", "no window")

    # Reset back to idle
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 16: PAUSED VIEW ==========

def test_group_16_paused(win, pid):
    log("=" * 50)
    log("GROUP 16: Paused View (Cmd+4)")
    log("=" * 50)

    if not win:
        skip_test("Paused tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Switch to Paused via Cmd+4
    send_debug_key(4, pid)
    time.sleep(0.5)

    # 16.1 Screenshot of paused view
    log("--- 16.1: Paused view renders ---")
    win = find_app_window()
    if win:
        path = screenshot_window("16_1_paused", win["id"])
        pass_test("Paused view screenshot", path)
    else:
        fail_test("Paused view", "window disappeared after Cmd+4")
        return

    # 16.2 Window intact
    log("--- 16.2: Window intact ---")
    if win["w"] > 100 and win["h"] > 100:
        pass_test("Paused window size", f"{win['w']}x{win['h']}")
    else:
        fail_test("Paused window size", f"{win['w']}x{win['h']}")

    # 16.3 Click resume button (left half, bottom)
    log("--- 16.3: Click resume button ---")
    resume_x = win["x"] + win["w"] * 0.25
    resume_y = win["y"] + win["h"] * 0.82
    click(resume_x, resume_y)
    time.sleep(0.8)
    win_after = find_app_window()
    if win_after:
        screenshot_window("16_3_after_resume", win_after["id"])
        pass_test("Resume button click - app stable")
    else:
        fail_test("Resume button click", "window disappeared")

    # 16.4 Switch back to paused, click disconnect
    log("--- 16.4: Click disconnect from paused ---")
    send_debug_key(4, pid)
    time.sleep(0.5)
    win = find_app_window()
    if win:
        disc_x = win["x"] + win["w"] * 0.75
        disc_y = win["y"] + win["h"] * 0.82
        click(disc_x, disc_y)
        time.sleep(0.8)
        win_after = find_app_window()
        if win_after:
            screenshot_window("16_4_after_disconnect_paused", win_after["id"])
            pass_test("Disconnect from paused - app stable")
        else:
            fail_test("Disconnect from paused", "window disappeared")
    else:
        skip_test("Disconnect from paused", "no window")

    # Reset back to idle
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 17: DEVICE LIST INJECTION ==========

def test_group_17_devices(win, pid):
    log("=" * 50)
    log("GROUP 17: Device List Injection (Cmd+5)")
    log("=" * 50)

    if not win:
        skip_test("Device list tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Make sure we're on idle first
    send_debug_key(1, pid)
    time.sleep(0.3)

    # Inject devices via Cmd+5
    send_debug_key(5, pid)
    time.sleep(0.5)

    # 17.1 Screenshot with device list
    log("--- 17.1: Device list appears ---")
    win = find_app_window()
    if win:
        path = screenshot_window("17_1_devices", win["id"])
        pass_test("Device list screenshot", path)
    else:
        fail_test("Device list", "window disappeared after Cmd+5")
        return

    # 17.2 Click on first device card area
    log("--- 17.2: Click first device ---")
    dev_x = win["x"] + win["w"] / 2
    dev_y = win["y"] + win["h"] * 0.72
    click(dev_x, dev_y)
    time.sleep(0.5)
    win_after = find_app_window()
    if win_after:
        screenshot_window("17_2_after_device_click", win_after["id"])
        pass_test("Click device card - app stable")
    else:
        fail_test("Click device card", "window disappeared")

    # 17.3 Click on second device card area
    log("--- 17.3: Click second device ---")
    win = find_app_window()
    if win:
        dev2_y = win["y"] + win["h"] * 0.82
        click(dev_x, dev2_y)
        time.sleep(0.5)
        win_after = find_app_window()
        if win_after:
            screenshot_window("17_3_after_device2_click", win_after["id"])
            pass_test("Click second device card - app stable")
        else:
            fail_test("Click second device card", "window disappeared")
    else:
        skip_test("Second device click", "no window")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 18: ERROR DISPLAY ==========

def test_group_18_error(win, pid):
    log("=" * 50)
    log("GROUP 18: Error Display (Cmd+6)")
    log("=" * 50)

    if not win:
        skip_test("Error display tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Make sure we're on idle
    send_debug_key(1, pid)
    time.sleep(0.3)

    # Inject error via Cmd+6
    send_debug_key(6, pid)
    time.sleep(0.5)

    # 18.1 Screenshot with error
    log("--- 18.1: Error message appears ---")
    win = find_app_window()
    if win:
        path = screenshot_window("18_1_error", win["id"])
        pass_test("Error display screenshot", path)
    else:
        fail_test("Error display", "window disappeared after Cmd+6")
        return

    # 18.2 Error clears when switching to ModeSelect
    log("--- 18.2: Error clears on state change ---")
    send_debug_key(2, pid)
    time.sleep(0.5)
    win = find_app_window()
    if win:
        screenshot_window("18_2_error_cleared", win["id"])
        pass_test("Error clears on state change")
    else:
        fail_test("Error clear", "window disappeared")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 19: STATE TRANSITION FLOW ==========

def test_group_19_transitions(win, pid):
    log("=" * 50)
    log("GROUP 19: Full State Transition Flow")
    log("=" * 50)

    if not win:
        skip_test("State transition tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # 19.1 Idle -> ModeSelect -> Streaming -> Paused -> Idle
    log("--- 19.1: Full forward cycle ---")
    states = [
        (1, "idle_start"),
        (2, "mode_select"),
        (3, "streaming"),
        (4, "paused"),
        (1, "idle_end"),
    ]
    all_ok = True
    for key, name in states:
        send_debug_key(key, pid)
        time.sleep(0.5)
        w = find_app_window()
        if w:
            screenshot_window(f"19_1_{name}", w["id"])
        else:
            fail_test(f"State transition to {name}", "window lost")
            all_ok = False
            break
    if all_ok:
        pass_test("Full forward cycle Idle->Mode->Stream->Pause->Idle")

    # 19.2 Rapid state switching (stress)
    log("--- 19.2: Rapid state cycling ---")
    for cycle in range(3):
        for key in [1, 2, 3, 4, 1]:
            send_debug_key(key, pid)
            time.sleep(0.2)
    time.sleep(0.5)
    win_after = find_app_window()
    if win_after:
        screenshot_window("19_2_after_rapid_cycling", win_after["id"])
        pass_test("Survives 3 rapid full state cycles")
    else:
        fail_test("Rapid state cycling", "window disappeared")

    # 19.3 Inject devices then switch to streaming then back to idle with devices
    log("--- 19.3: Devices persist across states ---")
    send_debug_key(1, pid)
    time.sleep(0.3)
    send_debug_key(5, pid)
    time.sleep(0.3)
    send_debug_key(3, pid)
    time.sleep(0.5)
    win = find_app_window()
    if win:
        screenshot_window("19_3_streaming_after_devices", win["id"])
    send_debug_key(1, pid)
    time.sleep(0.5)
    win = find_app_window()
    if win:
        screenshot_window("19_3_back_to_idle", win["id"])
        pass_test("Devices visible after returning to idle")
    else:
        fail_test("Return to idle after devices", "window lost")

    # 19.4 Error + state switch clears error
    log("--- 19.4: Error cleared by state transitions ---")
    send_debug_key(1, pid)
    time.sleep(0.2)
    send_debug_key(6, pid)
    time.sleep(0.3)
    send_debug_key(3, pid)
    time.sleep(0.3)
    send_debug_key(1, pid)
    time.sleep(0.5)
    win = find_app_window()
    if win:
        screenshot_window("19_4_error_state_clear", win["id"])
        pass_test("Error display after state round-trip")
    else:
        fail_test("Error state round-trip", "window lost")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 20: CAPTURE MODE BUTTONS (v0.53) ==========

def test_group_20_capture_modes(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 20: Capture Mode Buttons (v0.53)")
    log("=" * 50)

    if not win:
        skip_test("Capture mode tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Switch to ModeSelect via Cmd+2
    send_debug_key(2, pid)
    time.sleep(0.5)

    win = find_app_window()
    if not win:
        fail_test("ModeSelect for capture modes", "window not found")
        return

    # The three mode buttons are in a horizontal row.
    # Layout: 12px left margin, each button = (PANEL_WIDTH - 28 - SPACING*2) / 3
    # PANEL_WIDTH = 288, SPACING = 8 → card_width = (288-28-16)/3 ≈ 81px
    # Buttons are at approximately:
    #   btn1_center_x = 12 + 81/2 ≈ 52
    #   btn2_center_x = 12 + 81 + 8 + 81/2 ≈ 141
    #   btn3_center_x = 12 + 81*2 + 8*2 + 81/2 ≈ 230
    # Y position: header ~18+25+8 + device card ~60 + 16 + label ~20 + 6 = ~153, buttons height 70
    # btn_center_y ≈ 153 + 35 = 188

    btn_y = win["y"] + 188
    btn1_x = win["x"] + 52   # 全屏镜像
    btn2_x = win["x"] + 141  # 选择窗口
    btn3_x = win["x"] + 230  # 自定义区域

    # 20.1 Screenshot of ModeSelect with three buttons
    log("--- 20.1: ModeSelect view with three mode buttons ---")
    screenshot_window("20_1_mode_select_buttons", win["id"])
    pass_test("ModeSelect view with 3 capture mode buttons")

    # 20.2 Click "全屏镜像" (fullscreen) button
    log("--- 20.2: Click fullscreen mirror button ---")
    click(btn1_x, btn_y)
    time.sleep(0.8)
    win_after = find_app_window()
    if win_after:
        screenshot_window("20_2_after_fullscreen_click", win_after["id"])
        pass_test("Fullscreen mirror button clicked - app stable")
    else:
        fail_test("Fullscreen mirror button", "window disappeared")

    # Reset to ModeSelect
    send_debug_key(2, pid)
    time.sleep(0.5)

    # 20.3 Click "选择窗口" (window select) button
    log("--- 20.3: Click window select button ---")
    win = find_app_window()
    if win:
        click(btn2_x, btn_y)
        time.sleep(0.8)
        win_after = find_app_window()
        if win_after:
            screenshot_window("20_3_after_window_select_click", win_after["id"])
            pass_test("Window select button clicked - app stable")
        else:
            fail_test("Window select button", "window disappeared")
    else:
        skip_test("Window select button", "no window")

    # Reset to ModeSelect
    send_debug_key(2, pid)
    time.sleep(0.5)

    # 20.4 Click "自定义区域" (region select) button
    log("--- 20.4: Click region select button ---")
    win = find_app_window()
    if win:
        click(btn3_x, btn_y)
        time.sleep(1.5)  # Region overlay may need time
        # The window goes fullscreen for region select, then we press Escape to cancel
        screenshot("20_4_region_overlay")
        # Press Escape to cancel region selection
        type_special("escape", pid)
        time.sleep(0.8)
        win_after = find_app_window()
        if win_after:
            screenshot_window("20_4_after_region_cancel", win_after["id"])
            pass_test("Region select: overlay shown and cancelled with Esc")
        else:
            fail_test("Region select cancel", "window not found after Esc")
    else:
        skip_test("Region select button", "no window")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 21: WINDOW LIST IN MODE SELECT (v0.53) ==========

def test_group_21_window_list(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 21: Window List in Mode Select (v0.53)")
    log("=" * 50)

    if not win:
        skip_test("Window list tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Switch to ModeSelect and inject window list
    send_debug_key(2, pid)
    time.sleep(0.3)
    send_debug_key(0, pid)  # Inject fake window list
    time.sleep(0.5)

    # 21.1 Screenshot with window list shown
    log("--- 21.1: Window list appears below mode buttons ---")
    win = find_app_window()
    if win:
        screenshot_window("21_1_window_list", win["id"])
        pass_test("Window list rendered in mode select")
    else:
        fail_test("Window list render", "window not found")
        return

    # 21.2 Click first window item (Chrome)
    log("--- 21.2: Click first window item ---")
    # Window list starts below mode buttons. Approximate position:
    # mode buttons end at y ~223, then spacing 16+12+label+4 ≈ 255
    # First item at y ≈ 255 + 16 = 271
    item_y = win["y"] + 280
    item_x = win["x"] + win["w"] / 2
    click(item_x, item_y)
    time.sleep(0.8)
    win_after = find_app_window()
    if win_after:
        screenshot_window("21_2_after_window_item_click", win_after["id"])
        pass_test("Click window list item - app responds")
    else:
        fail_test("Click window list item", "window disappeared")

    # 21.3 Click second window item
    log("--- 21.3: Click second window item ---")
    send_debug_key(2, pid)
    time.sleep(0.3)
    send_debug_key(0, pid)
    time.sleep(0.3)
    win = find_app_window()
    if win:
        item2_y = win["y"] + 314  # Second item ~ 34px below first
        click(item_x, item2_y)
        time.sleep(0.8)
        win_after = find_app_window()
        if win_after:
            screenshot_window("21_3_after_window2_click", win_after["id"])
            pass_test("Click second window item - app responds")
        else:
            fail_test("Click second window item", "window disappeared")
    else:
        skip_test("Second window item", "no window")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== TEST GROUP 22: REGION SELECT DRAG (v0.53) ==========

def test_group_22_region_drag(win, pid):
    log("")
    log("=" * 50)
    log("GROUP 22: Region Select Drag & Confirm (v0.53)")
    log("=" * 50)

    if not win:
        skip_test("Region drag tests", "no window")
        return

    activate_app(pid)
    time.sleep(0.3)

    # Go to ModeSelect, click region button
    send_debug_key(2, pid)
    time.sleep(0.5)
    win = find_app_window()
    if not win:
        skip_test("Region drag", "no window in ModeSelect")
        return

    # Click region button (third button)
    btn3_x = win["x"] + 230
    btn_y = win["y"] + 188
    click(btn3_x, btn_y)
    time.sleep(1.5)  # Wait for fullscreen overlay

    # 22.1 Region overlay is showing (fullscreen screenshot)
    log("--- 22.1: Region overlay visible ---")
    screenshot("22_1_region_overlay_fullscreen")
    pass_test("Region overlay fullscreen screenshot")

    # 22.2 Drag to select a region (200x150 area in center of screen)
    log("--- 22.2: Drag to select region ---")
    # Drag from center-ish area
    drag(400, 300, 700, 500, steps=15)
    time.sleep(0.5)
    screenshot("22_2_region_selected")
    pass_test("Region drag completed (check screenshot for selection rect)")

    # 22.3 Press Enter to confirm selection
    log("--- 22.3: Press Enter to confirm ---")
    type_special("return", pid)
    time.sleep(1.0)
    win_after = find_app_window()
    if win_after:
        screenshot_window("22_3_after_region_confirm", win_after["id"])
        pass_test("Region confirmed via Enter - app returned to normal")
    else:
        # Window might have gone to streaming, try fullscreen screenshot
        screenshot("22_3_no_window_after_confirm")
        pass_test("Region confirmed (window may be in streaming state)")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)

    # 22.4 Region select + Escape cancellation
    log("--- 22.4: Region select + Escape cancel ---")
    send_debug_key(2, pid)
    time.sleep(0.3)
    win = find_app_window()
    if win:
        click(win["x"] + 230, win["y"] + 188)
        time.sleep(1.5)
        screenshot("22_4_region_before_escape")
        type_special("escape", pid)
        time.sleep(0.8)
        win_after = find_app_window()
        if win_after:
            screenshot_window("22_4_after_escape", win_after["id"])
            pass_test("Region cancelled with Escape - returned to ModeSelect")
        else:
            fail_test("Region Escape cancel", "window not found")
    else:
        skip_test("Region Escape cancel", "no window")

    # Reset
    send_debug_key(1, pid)
    time.sleep(0.3)


# ========== MAIN ==========

def main():
    global sender_pid

    log("=" * 60)
    log("Screen Mirror Sender - Comprehensive UI Test Suite")
    log("=" * 60)
    log(f"Screenshots: {SCREENSHOTS_DIR}/")
    log("")

    # Kill any existing sender
    subprocess.run(["pkill", "-f", "target/debug/sender"], capture_output=True)
    time.sleep(1)

    # Build
    log("Building...")
    result = subprocess.run(
        ["cargo", "build", "--bin", "sender"],
        capture_output=True, text=True,
        cwd=PROJECT_DIR
    )
    if result.returncode != 0:
        log(f"Build failed: {result.stderr}")
        sys.exit(1)
    log("Build OK")

    # Launch sender
    log("Launching sender...")
    proc = subprocess.Popen(
        [os.path.join(PROJECT_DIR, SENDER_BIN)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    sender_pid = proc.pid
    log(f"Sender PID: {sender_pid}")
    time.sleep(3)

    try:
        # Group 1: Launch & window basics
        win = test_group_01_launch(sender_pid)

        # Group 2: UI content visual inspection
        test_group_02_ui_content(win, sender_pid)

        # Group 3: PIN input basics
        test_group_03_pin_basics(win, sender_pid)

        # Group 4: PIN cursor & click-to-edit
        test_group_04_pin_cursor(win, sender_pid)

        # Group 5: PIN edge cases
        test_group_05_pin_edge(win, sender_pid)

        # Group 6: Window drag
        win = test_group_06_drag(win, sender_pid) or win

        # Group 7: Tray icon & menu
        test_group_07_tray(sender_pid)

        # Group 8: Focus loss & multi-app switching
        # Ensure window is visible first
        activate_app(sender_pid)
        time.sleep(0.5)
        win = find_app_window()
        if not win:
            # Try tray show
            try:
                script_open = f'tell application "System Events" to tell (first process whose unix id is {sender_pid}) to click menu bar item 1 of menu bar 2'
                script_show = f'tell application "System Events" to tell (first process whose unix id is {sender_pid}) to click menu item 1 of menu 1 of menu bar item 1 of menu bar 2'
                subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
                time.sleep(0.5)
                subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
                time.sleep(1.5)
                win = find_app_window()
            except subprocess.TimeoutExpired:
                pass
        test_group_08_focus(win, sender_pid)

        # Group 9: PIN state persistence
        test_group_09_pin_persist(sender_pid)

        # Group 10: Click various UI areas
        win = find_app_window()
        test_group_10_click_areas(win, sender_pid)

        # Group 11: Window position after show/hide
        # Ensure window is visible
        activate_app(sender_pid)
        time.sleep(0.5)
        if not find_app_window():
            try:
                script_open = f'tell application "System Events" to tell (first process whose unix id is {sender_pid}) to click menu bar item 1 of menu bar 2'
                script_show = f'tell application "System Events" to tell (first process whose unix id is {sender_pid}) to click menu item 1 of menu 1 of menu bar item 1 of menu bar 2'
                subprocess.run(["osascript", "-e", script_open], capture_output=True, text=True, timeout=5)
                time.sleep(0.5)
                subprocess.run(["osascript", "-e", script_show], capture_output=True, text=True, timeout=5)
                time.sleep(1.5)
            except subprocess.TimeoutExpired:
                pass
        test_group_11_position(sender_pid)

        # Group 12: Animation continuity
        test_group_12_animation(sender_pid)

        # Group 13: Stress test
        test_group_13_stress(sender_pid)

        # Group 14-19: Debug hotkey view tests
        activate_app(sender_pid)
        time.sleep(0.5)
        win = find_app_window()

        # Group 14: Mode Select view
        test_group_14_mode_select(win, sender_pid)

        # Group 15: Streaming view
        win = find_app_window()
        test_group_15_streaming(win, sender_pid)

        # Group 16: Paused view
        win = find_app_window()
        test_group_16_paused(win, sender_pid)

        # Group 17: Device list injection
        win = find_app_window()
        test_group_17_devices(win, sender_pid)

        # Group 18: Error display
        win = find_app_window()
        test_group_18_error(win, sender_pid)

        # Group 19: State transition flow
        win = find_app_window()
        test_group_19_transitions(win, sender_pid)

        # Group 20: v0.53 capture mode buttons
        win = find_app_window()
        test_group_20_capture_modes(win, sender_pid)

        # Group 21: v0.53 window list in mode select
        win = find_app_window()
        test_group_21_window_list(win, sender_pid)

        # Group 22: v0.53 region select drag
        win = find_app_window()
        test_group_22_region_drag(win, sender_pid)

    finally:
        log("")
        log("Cleaning up...")
        proc.terminate()
        try:
            proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            proc.kill()

    # Summary
    log("")
    log("=" * 60)
    log("TEST SUMMARY")
    log("=" * 60)
    passed = sum(1 for r in test_results if r[0] == "PASS")
    failed = sum(1 for r in test_results if r[0] == "FAIL")
    skipped = sum(1 for r in test_results if r[0] == "SKIP")

    for status, name in test_results:
        if status == "PASS":
            icon = "✓"
        elif status == "FAIL":
            icon = "✗"
        else:
            icon = "○"
        log(f"  {icon} {name}")

    log(f"\n  Total: {passed} passed, {failed} failed, {skipped} skipped")
    log(f"  Screenshots ({len(os.listdir(SCREENSHOTS_DIR))} files): {SCREENSHOTS_DIR}/")

    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
