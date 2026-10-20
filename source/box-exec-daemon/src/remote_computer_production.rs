use crate::remote_computer_backend::{
    RemoteComputerAction, RemoteComputerActionOutput, RemoteComputerExecutor,
};
use crate::server::RemoteRequestIdentity;
use fabushi_android_shared::ExecutionError;
use serde_json::Value;
use std::time::Duration;

pub trait DesktopComputerIo: Send {
    fn mouse_move(&mut self, x: i32, y: i32) -> Result<(), ExecutionError>;
    fn click(
        &mut self,
        x: Option<i32>,
        y: Option<i32>,
        button: &str,
        count: u32,
        modifier_keys: Option<&str>,
    ) -> Result<(), ExecutionError>;
    fn mouse_down(&mut self, button: &str) -> Result<(), ExecutionError>;
    fn mouse_up(&mut self, button: &str) -> Result<(), ExecutionError>;
    fn drag(
        &mut self,
        path: &[(i32, i32)],
        button: &str,
        modifier_keys: Option<&str>,
    ) -> Result<(), ExecutionError>;
    fn scroll(
        &mut self,
        x: Option<i32>,
        y: Option<i32>,
        direction: &str,
        amount: i32,
        modifier_keys: Option<&str>,
    ) -> Result<(), ExecutionError>;
    fn type_text(&mut self, text: &str) -> Result<(), ExecutionError>;
    fn key(&mut self, key: &str, hold_duration_ms: Option<u64>) -> Result<(), ExecutionError>;
    fn wait(&mut self, duration_ms: u64) -> Result<(), ExecutionError>;
    fn screenshot(&mut self) -> Result<String, ExecutionError>;
    fn cursor_position(&mut self) -> Result<(i32, i32), ExecutionError>;
}

#[derive(Default)]
pub struct SystemDesktopComputerIo;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod system {
    use super::*;
    use base64::Engine as _;
    use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
    use image::{DynamicImage, ImageFormat};
    use std::io::Cursor;
    use xcap::Monitor;

    fn input_error(error: impl std::fmt::Display) -> ExecutionError {
        ExecutionError::Transport(format!("remote Computer native input failed: {error}"))
    }

    fn capture_error(error: impl std::fmt::Display) -> ExecutionError {
        ExecutionError::Transport(format!("remote Computer native capture failed: {error}"))
    }

    fn enigo() -> Result<Enigo, ExecutionError> {
        Enigo::new(&Settings::default()).map_err(input_error)
    }

    fn mouse_button(raw: &str) -> Result<Button, ExecutionError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "unspecified" | "left" => Ok(Button::Left),
            "right" => Ok(Button::Right),
            "middle" => Ok(Button::Middle),
            "back" => Ok(Button::Back),
            "forward" => Ok(Button::Forward),
            other => Err(ExecutionError::InvalidRequest(format!(
                "unsupported remote Computer mouse button: {other}"
            ))),
        }
    }

    fn named_key(raw: &str) -> Result<Key, ExecutionError> {
        let normalized = raw.trim().to_ascii_lowercase();
        Ok(match normalized.as_str() {
            "ctrl" | "control" | "primary" => Key::Control,
            "shift" => Key::Shift,
            "alt" | "option" => Key::Alt,
            "meta" | "cmd" | "command" | "super" | "win" | "windows" => Key::Meta,
            "enter" | "return" => Key::Return,
            "tab" => Key::Tab,
            "escape" | "esc" => Key::Escape,
            "backspace" => Key::Backspace,
            "delete" | "del" => Key::Delete,
            "space" => Key::Space,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" | "page-up" => Key::PageUp,
            "pagedown" | "page-down" => Key::PageDown,
            "arrowup" | "up" => Key::UpArrow,
            "arrowdown" | "down" => Key::DownArrow,
            "arrowleft" | "left" => Key::LeftArrow,
            "arrowright" | "right" => Key::RightArrow,
            "f1" => Key::F1,
            "f2" => Key::F2,
            "f3" => Key::F3,
            "f4" => Key::F4,
            "f5" => Key::F5,
            "f6" => Key::F6,
            "f7" => Key::F7,
            "f8" => Key::F8,
            "f9" => Key::F9,
            "f10" => Key::F10,
            "f11" => Key::F11,
            "f12" => Key::F12,
            _ if normalized.chars().count() == 1 => {
                Key::Unicode(normalized.chars().next().unwrap())
            }
            _ => {
                return Err(ExecutionError::InvalidRequest(format!(
                    "unsupported remote Computer key: {raw}"
                )))
            }
        })
    }

    fn modifier_keys(raw: Option<&str>) -> Result<Vec<Key>, ExecutionError> {
        let Some(raw) = raw.filter(|raw| !raw.trim().is_empty()) else {
            return Ok(Vec::new());
        };
        raw.split(|character| character == '+' || character == ',' || character == ' ')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(named_key)
            .collect()
    }

    fn with_modifiers<T>(
        enigo: &mut Enigo,
        raw: Option<&str>,
        body: impl FnOnce(&mut Enigo) -> Result<T, ExecutionError>,
    ) -> Result<T, ExecutionError> {
        let modifiers = modifier_keys(raw)?;
        for key in &modifiers {
            enigo.key(*key, Direction::Press).map_err(input_error)?;
        }
        let result = body(enigo);
        for key in modifiers.iter().rev() {
            enigo.key(*key, Direction::Release).map_err(input_error)?;
        }
        result
    }

    fn optional_move(enigo: &mut Enigo, x: Option<i32>, y: Option<i32>) -> Result<(), ExecutionError> {
        match (x, y) {
            (Some(x), Some(y)) => enigo.move_mouse(x, y, Coordinate::Abs).map_err(input_error),
            (None, None) => Ok(()),
            _ => Err(ExecutionError::InvalidRequest(
                "remote Computer coordinate requires both x and y".into(),
            )),
        }
    }

    impl DesktopComputerIo for SystemDesktopComputerIo {
        fn mouse_move(&mut self, x: i32, y: i32) -> Result<(), ExecutionError> {
            enigo()?.move_mouse(x, y, Coordinate::Abs).map_err(input_error)
        }

        fn click(
            &mut self,
            x: Option<i32>,
            y: Option<i32>,
            button: &str,
            count: u32,
            modifiers: Option<&str>,
        ) -> Result<(), ExecutionError> {
            let mut enigo = enigo()?;
            optional_move(&mut enigo, x, y)?;
            let button = mouse_button(button)?;
            with_modifiers(&mut enigo, modifiers, |enigo| {
                for _ in 0..count.max(1) {
                    enigo.button(button, Direction::Click).map_err(input_error)?;
                }
                Ok(())
            })
        }

        fn mouse_down(&mut self, button: &str) -> Result<(), ExecutionError> {
            enigo()?.button(mouse_button(button)?, Direction::Press).map_err(input_error)
        }

        fn mouse_up(&mut self, button: &str) -> Result<(), ExecutionError> {
            enigo()?.button(mouse_button(button)?, Direction::Release).map_err(input_error)
        }

        fn drag(
            &mut self,
            path: &[(i32, i32)],
            button: &str,
            modifiers: Option<&str>,
        ) -> Result<(), ExecutionError> {
            if path.len() < 2 {
                return Err(ExecutionError::InvalidRequest(
                    "remote Computer drag requires at least two path coordinates".into(),
                ));
            }
            let mut enigo = enigo()?;
            let button = mouse_button(button)?;
            with_modifiers(&mut enigo, modifiers, |enigo| {
                enigo.move_mouse(path[0].0, path[0].1, Coordinate::Abs).map_err(input_error)?;
                enigo.button(button, Direction::Press).map_err(input_error)?;
                for &(x, y) in &path[1..] {
                    enigo.move_mouse(x, y, Coordinate::Abs).map_err(input_error)?;
                    std::thread::sleep(Duration::from_millis(10));
                }
                enigo.button(button, Direction::Release).map_err(input_error)
            })
        }

        fn scroll(
            &mut self,
            x: Option<i32>,
            y: Option<i32>,
            direction: &str,
            amount: i32,
            modifiers: Option<&str>,
        ) -> Result<(), ExecutionError> {
            let mut enigo = enigo()?;
            optional_move(&mut enigo, x, y)?;
            let magnitude = amount.abs().max(1);
            let (axis, value) = match direction.trim().to_ascii_lowercase().as_str() {
                "" | "unspecified" | "down" => (Axis::Vertical, magnitude),
                "up" => (Axis::Vertical, -magnitude),
                "right" => (Axis::Horizontal, magnitude),
                "left" => (Axis::Horizontal, -magnitude),
                other => {
                    return Err(ExecutionError::InvalidRequest(format!(
                        "unsupported remote Computer scroll direction: {other}"
                    )))
                }
            };
            with_modifiers(&mut enigo, modifiers, |enigo| {
                enigo.scroll(value, axis).map_err(input_error)
            })
        }

        fn type_text(&mut self, text: &str) -> Result<(), ExecutionError> {
            enigo()?.text(text).map_err(input_error)
        }

        fn key(&mut self, raw: &str, hold_duration_ms: Option<u64>) -> Result<(), ExecutionError> {
            let mut enigo = enigo()?;
            let keys = raw
                .split('+')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(named_key)
                .collect::<Result<Vec<_>, _>>()?;
            if keys.is_empty() {
                return Err(ExecutionError::InvalidRequest(
                    "remote Computer key chord is empty".into(),
                ));
            }
            for key in &keys[..keys.len().saturating_sub(1)] {
                enigo.key(*key, Direction::Press).map_err(input_error)?;
            }
            let last = keys[keys.len() - 1];
            if let Some(delay) = hold_duration_ms.filter(|delay| *delay > 0) {
                enigo.key(last, Direction::Press).map_err(input_error)?;
                std::thread::sleep(Duration::from_millis(delay));
                enigo.key(last, Direction::Release).map_err(input_error)?;
            } else {
                enigo.key(last, Direction::Click).map_err(input_error)?;
            }
            for key in keys[..keys.len().saturating_sub(1)].iter().rev() {
                enigo.key(*key, Direction::Release).map_err(input_error)?;
            }
            Ok(())
        }

        fn wait(&mut self, duration_ms: u64) -> Result<(), ExecutionError> {
            std::thread::sleep(Duration::from_millis(duration_ms));
            Ok(())
        }

        fn screenshot(&mut self) -> Result<String, ExecutionError> {
            let monitors = Monitor::all().map_err(capture_error)?;
            let monitor = monitors
                .iter()
                .find(|monitor| monitor.is_primary().unwrap_or(false))
                .or_else(|| monitors.first())
                .ok_or_else(|| ExecutionError::Transport("remote Computer has no monitor".into()))?;
            let image = monitor.capture_image().map_err(capture_error)?;
            let mut cursor = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(image)
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(capture_error)?;
            Ok(base64::engine::general_purpose::STANDARD.encode(cursor.into_inner()))
        }

        fn cursor_position(&mut self) -> Result<(i32, i32), ExecutionError> {
            enigo()?.location().map_err(input_error)
        }
    }
}


#[cfg(target_os = "macos")]
mod macos_system {
    use super::*;
    use base64::Engine as _;
    use core_foundation::base::TCFType;
    use core_foundation::data::CFData;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{kCFAllocatorDefault, CFRelease, CFTypeRef};
    use core_foundation_sys::data::{CFDataCreateMutable, CFDataRef, CFMutableDataRef};
    use core_foundation_sys::dictionary::CFDictionaryRef;
    use core_foundation_sys::string::CFStringRef;
    use core_graphics::display::CGDisplay;
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField, KeyCode,
        ScrollEventUnit,
    };
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
    use core_graphics::geometry::CGPoint;
    use foreign_types::ForeignType;
    use std::ffi::c_void;
    use std::ptr;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFGetTypeID(value: CFTypeRef) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(value: *const c_void) -> u8;
        fn CFDictionaryGetValue(dictionary: CFDictionaryRef, key: *const c_void) -> *const c_void;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        fn CGImageDestinationCreateWithData(
            data: CFMutableDataRef,
            type_identifier: CFStringRef,
            count: usize,
            options: CFDictionaryRef,
        ) -> *mut c_void;
        fn CGImageDestinationAddImage(
            destination: *mut c_void,
            image: *mut c_void,
            properties: CFDictionaryRef,
        );
        fn CGImageDestinationFinalize(destination: *mut c_void) -> u8;
    }

    fn input_error(message: impl Into<String>) -> ExecutionError {
        ExecutionError::Transport(format!("remote Computer macOS native input failed: {}", message.into()))
    }

    fn capture_error(message: impl Into<String>) -> ExecutionError {
        ExecutionError::Transport(format!("remote Computer macOS native capture failed: {}", message.into()))
    }

    fn permission_error(message: impl Into<String>) -> ExecutionError {
        ExecutionError::CapabilityUnavailable(format!("remote Computer macOS permission unavailable: {}", message.into()))
    }

    fn session_boolean(dictionary: CFDictionaryRef, key: *const c_void) -> Option<bool> {
        if dictionary.is_null() || key.is_null() {
            return None;
        }
        let value = unsafe { CFDictionaryGetValue(dictionary, key) };
        if value.is_null() || unsafe { CFGetTypeID(value as CFTypeRef) } != unsafe { CFBooleanGetTypeID() } {
            return None;
        }
        Some(unsafe { CFBooleanGetValue(value) != 0 })
    }

    fn ensure_interactive_session() -> Result<(), ExecutionError> {
        let dictionary = unsafe { CGSessionCopyCurrentDictionary() };
        if dictionary.is_null() {
            return Err(permission_error("an unlocked interactive session is required"));
        }
        let on_console_key = CFString::new("kCGSessionOnConsoleKey");
        let login_done_key = CFString::new("kCGSessionLoginDoneKey");
        let locked_key = CFString::new("CGSSessionScreenIsLocked");
        let on_console = session_boolean(
            dictionary,
            on_console_key.as_concrete_TypeRef() as *const c_void,
        )
        .unwrap_or(false);
        let login_done = session_boolean(
            dictionary,
            login_done_key.as_concrete_TypeRef() as *const c_void,
        )
        .unwrap_or(false);
        let locked = session_boolean(
            dictionary,
            locked_key.as_concrete_TypeRef() as *const c_void,
        )
        .unwrap_or(false);
        unsafe { CFRelease(dictionary as CFTypeRef) };
        if !on_console || !login_done || locked {
            return Err(permission_error("the interactive session is locked or unavailable"));
        }
        Ok(())
    }

    fn require_accessibility() -> Result<(), ExecutionError> {
        ensure_interactive_session()?;
        if !unsafe { AXIsProcessTrusted() } {
            return Err(permission_error(
                "Accessibility permission is required in System Settings > Privacy & Security",
            ));
        }
        Ok(())
    }

    fn require_screen_recording() -> Result<(), ExecutionError> {
        ensure_interactive_session()?;
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            return Err(permission_error(
                "Screen Recording permission is required in System Settings > Privacy & Security",
            ));
        }
        Ok(())
    }

    fn event_source() -> Result<CGEventSource, ExecutionError> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| input_error("could not create CoreGraphics event source"))
    }

    fn cursor_point() -> Result<CGPoint, ExecutionError> {
        CGEvent::new(event_source()?)
            .map(|event| event.location())
            .map_err(|_| input_error("could not read cursor location"))
    }

    fn position(x: Option<i32>, y: Option<i32>) -> Result<CGPoint, ExecutionError> {
        match (x, y) {
            (Some(x), Some(y)) => Ok(CGPoint::new(f64::from(x), f64::from(y))),
            (None, None) => cursor_point(),
            _ => Err(ExecutionError::InvalidRequest(
                "remote Computer coordinate requires both x and y".into(),
            )),
        }
    }

    fn mouse_button(raw: &str) -> Result<CGMouseButton, ExecutionError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "unspecified" | "left" => Ok(CGMouseButton::Left),
            "right" => Ok(CGMouseButton::Right),
            "middle" => Ok(CGMouseButton::Center),
            "back" | "forward" => Err(ExecutionError::InvalidRequest(
                "macOS CoreGraphics shipping authority supports left/right/middle mouse buttons".into(),
            )),
            other => Err(ExecutionError::InvalidRequest(format!(
                "unsupported remote Computer mouse button: {other}"
            ))),
        }
    }

    fn mouse_types(button: CGMouseButton) -> (CGEventType, CGEventType, CGEventType) {
        match button {
            CGMouseButton::Left => (
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseDragged,
                CGEventType::LeftMouseUp,
            ),
            CGMouseButton::Right => (
                CGEventType::RightMouseDown,
                CGEventType::RightMouseDragged,
                CGEventType::RightMouseUp,
            ),
            CGMouseButton::Center => (
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseDragged,
                CGEventType::OtherMouseUp,
            ),
        }
    }

    fn modifier_flags(raw: Option<&str>) -> Result<CGEventFlags, ExecutionError> {
        let mut flags = CGEventFlags::empty();
        let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
            return Ok(flags);
        };
        for part in raw
            .split(|character| character == '+' || character == ',' || character == ' ')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => flags |= CGEventFlags::CGEventFlagControl,
                "alt" | "option" => flags |= CGEventFlags::CGEventFlagAlternate,
                "shift" => flags |= CGEventFlags::CGEventFlagShift,
                "cmd" | "command" | "meta" | "super" | "primary" => {
                    flags |= CGEventFlags::CGEventFlagCommand
                }
                other => {
                    return Err(ExecutionError::InvalidRequest(format!(
                        "unsupported remote Computer modifier: {other}"
                    )))
                }
            }
        }
        Ok(flags)
    }

    fn post_mouse(
        event_type: CGEventType,
        position: CGPoint,
        button: CGMouseButton,
        click_count: Option<u32>,
        flags: CGEventFlags,
    ) -> Result<(), ExecutionError> {
        let event = CGEvent::new_mouse_event(event_source()?, event_type, position, button)
            .map_err(|_| input_error("could not create CoreGraphics mouse event"))?;
        if let Some(count) = click_count {
            event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, i64::from(count));
        }
        event.set_flags(flags);
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn move_to(position: CGPoint, dragged: Option<CGMouseButton>, flags: CGEventFlags) -> Result<(), ExecutionError> {
        let event_type = dragged.map_or(CGEventType::MouseMoved, |button| mouse_types(button).1);
        post_mouse(
            event_type,
            position,
            dragged.unwrap_or(CGMouseButton::Left),
            None,
            flags,
        )
    }

    fn unicode_chunks(text: &str, max_utf16: usize) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut units = 0usize;
        for character in text.chars() {
            let next = character.len_utf16();
            if !current.is_empty() && units + next > max_utf16 {
                chunks.push(std::mem::take(&mut current));
                units = 0;
            }
            current.push(character);
            units += next;
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        chunks
    }

    fn key_code(raw: &str) -> Option<u16> {
        let key = raw.to_ascii_lowercase();
        Some(match key.as_str() {
            "a" => KeyCode::ANSI_A, "b" => KeyCode::ANSI_B, "c" => KeyCode::ANSI_C,
            "d" => KeyCode::ANSI_D, "e" => KeyCode::ANSI_E, "f" => KeyCode::ANSI_F,
            "g" => KeyCode::ANSI_G, "h" => KeyCode::ANSI_H, "i" => KeyCode::ANSI_I,
            "j" => KeyCode::ANSI_J, "k" => KeyCode::ANSI_K, "l" => KeyCode::ANSI_L,
            "m" => KeyCode::ANSI_M, "n" => KeyCode::ANSI_N, "o" => KeyCode::ANSI_O,
            "p" => KeyCode::ANSI_P, "q" => KeyCode::ANSI_Q, "r" => KeyCode::ANSI_R,
            "s" => KeyCode::ANSI_S, "t" => KeyCode::ANSI_T, "u" => KeyCode::ANSI_U,
            "v" => KeyCode::ANSI_V, "w" => KeyCode::ANSI_W, "x" => KeyCode::ANSI_X,
            "y" => KeyCode::ANSI_Y, "z" => KeyCode::ANSI_Z,
            "0" => KeyCode::ANSI_0, "1" => KeyCode::ANSI_1, "2" => KeyCode::ANSI_2,
            "3" => KeyCode::ANSI_3, "4" => KeyCode::ANSI_4, "5" => KeyCode::ANSI_5,
            "6" => KeyCode::ANSI_6, "7" => KeyCode::ANSI_7, "8" => KeyCode::ANSI_8,
            "9" => KeyCode::ANSI_9,
            "return" | "enter" => KeyCode::RETURN,
            "tab" => KeyCode::TAB,
            "space" | "spacebar" => KeyCode::SPACE,
            "backspace" => KeyCode::DELETE,
            "delete" | "forwarddelete" => KeyCode::FORWARD_DELETE,
            "escape" | "esc" => KeyCode::ESCAPE,
            "left" | "arrowleft" => KeyCode::LEFT_ARROW,
            "right" | "arrowright" => KeyCode::RIGHT_ARROW,
            "up" | "arrowup" => KeyCode::UP_ARROW,
            "down" | "arrowdown" => KeyCode::DOWN_ARROW,
            "home" => KeyCode::HOME, "end" => KeyCode::END,
            "pageup" | "page-up" => KeyCode::PAGE_UP,
            "pagedown" | "page-down" => KeyCode::PAGE_DOWN,
            "f1" => KeyCode::F1, "f2" => KeyCode::F2, "f3" => KeyCode::F3, "f4" => KeyCode::F4,
            "f5" => KeyCode::F5, "f6" => KeyCode::F6, "f7" => KeyCode::F7, "f8" => KeyCode::F8,
            "f9" => KeyCode::F9, "f10" => KeyCode::F10, "f11" => KeyCode::F11, "f12" => KeyCode::F12,
            "-" | "minus" => KeyCode::ANSI_MINUS,
            "=" | "equal" => KeyCode::ANSI_EQUAL,
            "," | "comma" => KeyCode::ANSI_COMMA,
            "." | "period" => KeyCode::ANSI_PERIOD,
            "/" | "slash" => KeyCode::ANSI_SLASH,
            ";" | "semicolon" => KeyCode::ANSI_SEMICOLON,
            "'" | "quote" => KeyCode::ANSI_QUOTE,
            "[" | "leftbracket" => KeyCode::ANSI_LEFT_BRACKET,
            "]" | "rightbracket" => KeyCode::ANSI_RIGHT_BRACKET,
            "\\" | "backslash" => KeyCode::ANSI_BACKSLASH,
            "`" | "grave" => KeyCode::ANSI_GRAVE,
            _ => return None,
        })
    }

    impl DesktopComputerIo for SystemDesktopComputerIo {
        fn mouse_move(&mut self, x: i32, y: i32) -> Result<(), ExecutionError> {
            require_accessibility()?;
            move_to(CGPoint::new(f64::from(x), f64::from(y)), None, CGEventFlags::empty())
        }

        fn click(
            &mut self,
            x: Option<i32>,
            y: Option<i32>,
            button: &str,
            count: u32,
            modifiers: Option<&str>,
        ) -> Result<(), ExecutionError> {
            require_accessibility()?;
            let position = position(x, y)?;
            let button = mouse_button(button)?;
            let flags = modifier_flags(modifiers)?;
            let (down, _, up) = mouse_types(button);
            let count = count.max(1).min(3);
            move_to(position, None, flags)?;
            for click_number in 1..=count {
                post_mouse(down, position, button, Some(click_number), flags)?;
                post_mouse(up, position, button, Some(click_number), flags)?;
                if click_number < count {
                    std::thread::sleep(Duration::from_millis(60));
                }
            }
            Ok(())
        }

        fn mouse_down(&mut self, button: &str) -> Result<(), ExecutionError> {
            require_accessibility()?;
            let button = mouse_button(button)?;
            let position = cursor_point()?;
            post_mouse(mouse_types(button).0, position, button, Some(1), CGEventFlags::empty())
        }

        fn mouse_up(&mut self, button: &str) -> Result<(), ExecutionError> {
            require_accessibility()?;
            let button = mouse_button(button)?;
            let position = cursor_point()?;
            post_mouse(mouse_types(button).2, position, button, Some(1), CGEventFlags::empty())
        }

        fn drag(
            &mut self,
            path: &[(i32, i32)],
            button: &str,
            modifiers: Option<&str>,
        ) -> Result<(), ExecutionError> {
            require_accessibility()?;
            if path.len() < 2 {
                return Err(ExecutionError::InvalidRequest(
                    "remote Computer drag requires at least two path coordinates".into(),
                ));
            }
            let button = mouse_button(button)?;
            let flags = modifier_flags(modifiers)?;
            let (down, _, up) = mouse_types(button);
            let first = CGPoint::new(f64::from(path[0].0), f64::from(path[0].1));
            move_to(first, None, flags)?;
            post_mouse(down, first, button, Some(1), flags)?;
            std::thread::sleep(Duration::from_millis(20));
            for &(x, y) in &path[1..] {
                move_to(CGPoint::new(f64::from(x), f64::from(y)), Some(button), flags)?;
                std::thread::sleep(Duration::from_millis(12));
            }
            let (x, y) = path[path.len() - 1];
            post_mouse(
                up,
                CGPoint::new(f64::from(x), f64::from(y)),
                button,
                Some(1),
                flags,
            )
        }

        fn scroll(
            &mut self,
            x: Option<i32>,
            y: Option<i32>,
            direction: &str,
            amount: i32,
            modifiers: Option<&str>,
        ) -> Result<(), ExecutionError> {
            require_accessibility()?;
            match (x, y) {
                (Some(x), Some(y)) => move_to(
                    CGPoint::new(f64::from(x), f64::from(y)),
                    None,
                    CGEventFlags::empty(),
                )?,
                (None, None) => {}
                _ => {
                    return Err(ExecutionError::InvalidRequest(
                        "remote Computer coordinate requires both x and y".into(),
                    ))
                }
            }
            let magnitude = amount.saturating_abs().max(1);
            let (vertical, horizontal) = match direction.trim().to_ascii_lowercase().as_str() {
                "" | "unspecified" | "down" => (-magnitude, 0),
                "up" => (magnitude, 0),
                "left" => (0, magnitude),
                "right" => (0, -magnitude),
                other => {
                    return Err(ExecutionError::InvalidRequest(format!(
                        "unsupported remote Computer scroll direction: {other}"
                    )))
                }
            };
            let event = CGEvent::new_scroll_event(
                event_source()?,
                ScrollEventUnit::LINE,
                2,
                vertical,
                horizontal,
                0,
            )
            .map_err(|_| input_error("could not create CoreGraphics scroll event"))?;
            event.set_flags(modifier_flags(modifiers)?);
            event.post(CGEventTapLocation::HID);
            Ok(())
        }

        fn type_text(&mut self, text: &str) -> Result<(), ExecutionError> {
            require_accessibility()?;
            for chunk in unicode_chunks(text, 20) {
                let down = CGEvent::new_keyboard_event(event_source()?, 0, true)
                    .map_err(|_| input_error("could not create CoreGraphics keyboard event"))?;
                down.set_string(&chunk);
                down.post(CGEventTapLocation::HID);
                let up = CGEvent::new_keyboard_event(event_source()?, 0, false)
                    .map_err(|_| input_error("could not create CoreGraphics keyboard event"))?;
                up.post(CGEventTapLocation::HID);
            }
            Ok(())
        }

        fn key(&mut self, chord: &str, hold_duration_ms: Option<u64>) -> Result<(), ExecutionError> {
            require_accessibility()?;
            let parts = chord
                .split('+')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let Some(raw_key) = parts.last() else {
                return Err(ExecutionError::InvalidRequest(
                    "remote Computer key chord is empty".into(),
                ));
            };
            let flags = modifier_flags(Some(&parts[..parts.len().saturating_sub(1)].join("+")))?;
            let keycode = key_code(raw_key).ok_or_else(|| {
                ExecutionError::InvalidRequest(format!("unsupported remote Computer key: {raw_key}"))
            })?;
            let down = CGEvent::new_keyboard_event(event_source()?, keycode, true)
                .map_err(|_| input_error("could not create CoreGraphics key-down event"))?;
            down.set_flags(flags);
            down.post(CGEventTapLocation::HID);
            if let Some(delay) = hold_duration_ms.filter(|delay| *delay > 0) {
                std::thread::sleep(Duration::from_millis(delay));
            }
            let up = CGEvent::new_keyboard_event(event_source()?, keycode, false)
                .map_err(|_| input_error("could not create CoreGraphics key-up event"))?;
            up.set_flags(flags);
            up.post(CGEventTapLocation::HID);
            Ok(())
        }

        fn wait(&mut self, duration_ms: u64) -> Result<(), ExecutionError> {
            std::thread::sleep(Duration::from_millis(duration_ms));
            Ok(())
        }

        fn screenshot(&mut self) -> Result<String, ExecutionError> {
            require_screen_recording()?;
            let image = CGDisplay::main()
                .image()
                .ok_or_else(|| capture_error("CoreGraphics could not capture the main display"))?;
            let data_ref = unsafe { CFDataCreateMutable(kCFAllocatorDefault, 0) };
            if data_ref.is_null() {
                return Err(capture_error("ImageIO could not allocate the encoded screen buffer"));
            }
            let type_identifier = CFString::new("public.jpeg");
            let destination = unsafe {
                CGImageDestinationCreateWithData(
                    data_ref,
                    type_identifier.as_concrete_TypeRef(),
                    1,
                    ptr::null(),
                )
            };
            if destination.is_null() {
                unsafe { CFRelease(data_ref as CFTypeRef) };
                return Err(capture_error("ImageIO could not create the screen encoder"));
            }
            unsafe {
                CGImageDestinationAddImage(destination, image.as_ptr() as *mut c_void, ptr::null());
            }
            let finalized = unsafe { CGImageDestinationFinalize(destination) };
            unsafe { CFRelease(destination as CFTypeRef) };
            if finalized == 0 {
                unsafe { CFRelease(data_ref as CFTypeRef) };
                return Err(capture_error("ImageIO could not finalize the encoded screen"));
            }
            let data = unsafe { CFData::wrap_under_create_rule(data_ref as CFDataRef) };
            if data.is_empty() {
                return Err(capture_error("screen capture was empty"));
            }
            Ok(base64::engine::general_purpose::STANDARD.encode(data.bytes()))
        }

        fn cursor_position(&mut self) -> Result<(i32, i32), ExecutionError> {
            require_accessibility()?;
            let point = cursor_point()?;
            if !point.x.is_finite() || !point.y.is_finite() {
                return Err(input_error("CoreGraphics returned a non-finite cursor location"));
            }
            if point.x < f64::from(i32::MIN)
                || point.x > f64::from(i32::MAX)
                || point.y < f64::from(i32::MIN)
                || point.y > f64::from(i32::MAX)
            {
                return Err(input_error("CoreGraphics cursor location exceeds protocol range"));
            }
            Ok((point.x.round() as i32, point.y.round() as i32))
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
impl DesktopComputerIo for SystemDesktopComputerIo {
    fn mouse_move(&mut self, _x: i32, _y: i32) -> Result<(), ExecutionError> { unavailable() }
    fn click(&mut self, _x: Option<i32>, _y: Option<i32>, _button: &str, _count: u32, _modifier_keys: Option<&str>) -> Result<(), ExecutionError> { unavailable() }
    fn mouse_down(&mut self, _button: &str) -> Result<(), ExecutionError> { unavailable() }
    fn mouse_up(&mut self, _button: &str) -> Result<(), ExecutionError> { unavailable() }
    fn drag(&mut self, _path: &[(i32, i32)], _button: &str, _modifier_keys: Option<&str>) -> Result<(), ExecutionError> { unavailable() }
    fn scroll(&mut self, _x: Option<i32>, _y: Option<i32>, _direction: &str, _amount: i32, _modifier_keys: Option<&str>) -> Result<(), ExecutionError> { unavailable() }
    fn type_text(&mut self, _text: &str) -> Result<(), ExecutionError> { unavailable() }
    fn key(&mut self, _key: &str, _hold_duration_ms: Option<u64>) -> Result<(), ExecutionError> { unavailable() }
    fn wait(&mut self, _duration_ms: u64) -> Result<(), ExecutionError> { unavailable() }
    fn screenshot(&mut self) -> Result<String, ExecutionError> { unavailable() }
    fn cursor_position(&mut self) -> Result<(i32, i32), ExecutionError> { unavailable() }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn unavailable<T>() -> Result<T, ExecutionError> {
    Err(ExecutionError::CapabilityUnavailable(
        "remote Computer native executor requires a desktop Authorized Remote Runner".into(),
    ))
}

pub struct ProductionRemoteComputerExecutor<I = SystemDesktopComputerIo> {
    io: I,
}

impl ProductionRemoteComputerExecutor<SystemDesktopComputerIo> {
    pub fn new() -> Self {
        Self { io: SystemDesktopComputerIo }
    }
}

impl Default for ProductionRemoteComputerExecutor<SystemDesktopComputerIo> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: DesktopComputerIo> ProductionRemoteComputerExecutor<I> {
    pub fn with_io(io: I) -> Self {
        Self { io }
    }

    pub fn into_io(self) -> I {
        self.io
    }
}

fn object_i32(value: &Value, key: &str) -> Result<Option<i32>, ExecutionError> {
    value.get(key).and_then(Value::as_i64).map(|value| {
        i32::try_from(value).map_err(|_| ExecutionError::InvalidRequest(format!(
            "remote Computer {key} is outside native desktop range"
        )))
    }).transpose()
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn drag_path(value: &Value) -> Result<Vec<(i32, i32)>, ExecutionError> {
    let Some(path) = value.get("path").and_then(Value::as_array) else {
        return Err(ExecutionError::InvalidRequest(
            "remote Computer drag requires path".into(),
        ));
    };
    path.iter().map(|point| {
        let x = object_i32(point, "x")?.ok_or_else(|| ExecutionError::InvalidRequest(
            "remote Computer drag point requires x".into(),
        ))?;
        let y = object_i32(point, "y")?.ok_or_else(|| ExecutionError::InvalidRequest(
            "remote Computer drag point requires y".into(),
        ))?;
        Ok((x, y))
    }).collect()
}

impl<I: DesktopComputerIo> RemoteComputerExecutor for ProductionRemoteComputerExecutor<I> {
    fn execute_action(
        &mut self,
        _identity: &RemoteRequestIdentity,
        action: &RemoteComputerAction,
    ) -> Result<RemoteComputerActionOutput, ExecutionError> {
        let mut output = RemoteComputerActionOutput::default();
        match action {
            RemoteComputerAction::MouseMove { x, y } => {
                self.io.mouse_move(
                    i32::try_from(*x).map_err(|_| ExecutionError::InvalidRequest("remote Computer x is outside native desktop range".into()))?,
                    i32::try_from(*y).map_err(|_| ExecutionError::InvalidRequest("remote Computer y is outside native desktop range".into()))?,
                )?;
            }
            RemoteComputerAction::Click { arguments } => {
                self.io.click(
                    object_i32(arguments, "x")?,
                    object_i32(arguments, "y")?,
                    string(arguments, "button").unwrap_or("left"),
                    arguments.get("count").and_then(Value::as_u64).unwrap_or(1).try_into()
                        .map_err(|_| ExecutionError::InvalidRequest("remote Computer click count is too large".into()))?,
                    string(arguments, "modifierKeys"),
                )?;
            }
            RemoteComputerAction::MouseDown { arguments } => {
                self.io.mouse_down(string(arguments, "button").unwrap_or("left"))?;
            }
            RemoteComputerAction::MouseUp { arguments } => {
                self.io.mouse_up(string(arguments, "button").unwrap_or("left"))?;
            }
            RemoteComputerAction::Drag { arguments } => {
                let path = drag_path(arguments)?;
                self.io.drag(
                    &path,
                    string(arguments, "button").unwrap_or("left"),
                    string(arguments, "modifierKeys"),
                )?;
            }
            RemoteComputerAction::Scroll { arguments } => {
                self.io.scroll(
                    object_i32(arguments, "x")?,
                    object_i32(arguments, "y")?,
                    string(arguments, "direction").unwrap_or("down"),
                    object_i32(arguments, "amount")?.unwrap_or(1),
                    string(arguments, "modifierKeys"),
                )?;
            }
            RemoteComputerAction::Type { text } => self.io.type_text(text)?,
            RemoteComputerAction::Key { key, arguments } => {
                self.io.key(key, arguments.get("holdDurationMs").and_then(Value::as_u64))?;
            }
            RemoteComputerAction::Wait { duration_ms } => self.io.wait(*duration_ms)?,
            RemoteComputerAction::Screenshot => output.screenshot = Some(self.io.screenshot()?),
            RemoteComputerAction::CursorPosition => {
                let (x, y) = self.io.cursor_position()?;
                output.cursor_position = Some((i64::from(x), i64::from(y)));
            }
        }
        output.log = Some(format!("native remote Computer action executed: {}", action.wire_name()));
        Ok(output)
    }

    fn settle_after_screenshot(&mut self, delay_ms: u64) -> Result<(), ExecutionError> {
        self.io.wait(delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeDesktopIo {
        calls: Vec<String>,
    }

    impl DesktopComputerIo for FakeDesktopIo {
        fn mouse_move(&mut self, x: i32, y: i32) -> Result<(), ExecutionError> { self.calls.push(format!("move:{x}:{y}")); Ok(()) }
        fn click(&mut self, x: Option<i32>, y: Option<i32>, button: &str, count: u32, modifier_keys: Option<&str>) -> Result<(), ExecutionError> { self.calls.push(format!("click:{x:?}:{y:?}:{button}:{count}:{modifier_keys:?}")); Ok(()) }
        fn mouse_down(&mut self, button: &str) -> Result<(), ExecutionError> { self.calls.push(format!("down:{button}")); Ok(()) }
        fn mouse_up(&mut self, button: &str) -> Result<(), ExecutionError> { self.calls.push(format!("up:{button}")); Ok(()) }
        fn drag(&mut self, path: &[(i32, i32)], button: &str, modifier_keys: Option<&str>) -> Result<(), ExecutionError> { self.calls.push(format!("drag:{}:{button}:{modifier_keys:?}", path.len())); Ok(()) }
        fn scroll(&mut self, x: Option<i32>, y: Option<i32>, direction: &str, amount: i32, modifier_keys: Option<&str>) -> Result<(), ExecutionError> { self.calls.push(format!("scroll:{x:?}:{y:?}:{direction}:{amount}:{modifier_keys:?}")); Ok(()) }
        fn type_text(&mut self, text: &str) -> Result<(), ExecutionError> { self.calls.push(format!("type:{text}")); Ok(()) }
        fn key(&mut self, key: &str, hold_duration_ms: Option<u64>) -> Result<(), ExecutionError> { self.calls.push(format!("key:{key}:{hold_duration_ms:?}")); Ok(()) }
        fn wait(&mut self, duration_ms: u64) -> Result<(), ExecutionError> { self.calls.push(format!("wait:{duration_ms}")); Ok(()) }
        fn screenshot(&mut self) -> Result<String, ExecutionError> { self.calls.push("screenshot".into()); Ok("png-base64".into()) }
        fn cursor_position(&mut self) -> Result<(i32, i32), ExecutionError> { self.calls.push("cursor".into()); Ok((13, 17)) }
    }

    fn identity() -> RemoteRequestIdentity {
        RemoteRequestIdentity {
            credential_id: "cred".into(),
            operation_id: "op".into(),
            request_id: "req".into(),
            account_fence: "acct".into(),
            account_epoch: 9,
            permission_grant_id: "grant".into(),
            device_id: "device".into(),
        }
    }

    #[test]
    fn production_executor_maps_all_eleven_actions_to_native_io_and_returns_observation_fields() {
        let mut executor = ProductionRemoteComputerExecutor::with_io(FakeDesktopIo::default());
        let actions = vec![
            RemoteComputerAction::MouseMove { x: 1, y: 2 },
            RemoteComputerAction::Click { arguments: serde_json::json!({"x":3,"y":4,"button":"back","count":2,"modifierKeys":"shift"}) },
            RemoteComputerAction::MouseDown { arguments: serde_json::json!({"button":"left"}) },
            RemoteComputerAction::MouseUp { arguments: serde_json::json!({"button":"left"}) },
            RemoteComputerAction::Drag { arguments: serde_json::json!({"path":[{"x":1,"y":2},{"x":3,"y":4}],"button":"left","modifierKeys":"shift"}) },
            RemoteComputerAction::Scroll { arguments: serde_json::json!({"x":5,"y":6,"direction":"up","amount":120,"modifierKeys":"ctrl"}) },
            RemoteComputerAction::Type { text: "hello".into() },
            RemoteComputerAction::Key { key: "CTRL+ENTER".into(), arguments: serde_json::json!({"holdDurationMs":50}) },
            RemoteComputerAction::Wait { duration_ms: 10 },
            RemoteComputerAction::Screenshot,
            RemoteComputerAction::CursorPosition,
        ];
        let mut screenshot = None;
        let mut cursor = None;
        for action in &actions {
            let output = executor.execute_action(&identity(), action).unwrap();
            screenshot = output.screenshot.or(screenshot);
            cursor = output.cursor_position.or(cursor);
        }
        let io = executor.into_io();
        assert_eq!(io.calls.len(), 11);
        assert_eq!(screenshot.as_deref(), Some("png-base64"));
        assert_eq!(cursor, Some((13, 17)));
        assert!(io.calls.iter().any(|call| call.starts_with("click:Some(3):Some(4):back:2")));
        assert!(io.calls.iter().any(|call| call == "screenshot"));
        assert!(io.calls.iter().any(|call| call == "cursor"));
    }

    #[test]
    fn production_executor_routes_screenshot_settle_to_native_wait_without_replaying_actions() {
        let mut executor = ProductionRemoteComputerExecutor::with_io(FakeDesktopIo::default());
        let screenshot = RemoteComputerAction::Screenshot;
        let output = executor.execute_action(&identity(), &screenshot).unwrap();
        assert_eq!(output.screenshot.as_deref(), Some("png-base64"));
        executor.settle_after_screenshot(2_000).unwrap();
        let io = executor.into_io();
        assert_eq!(io.calls, vec!["screenshot", "wait:2000"]);
    }

    #[test]
    fn production_executor_cancel_and_reconcile_fail_safe_without_native_replay() {
        let mut executor = ProductionRemoteComputerExecutor::with_io(FakeDesktopIo::default());
        let request = crate::remote_computer_backend::RemoteComputerRequest {
            tool: "Computer".into(),
            actions: vec![RemoteComputerAction::Click {
                arguments: serde_json::json!({"x":1,"y":2}),
            }],
        };
        assert!(matches!(
            executor.reconcile(&identity(), &request).unwrap(),
            crate::server::RemoteBackendReconcileOutcome::Pending
        ));
        assert!(matches!(
            executor.cancel(&identity(), &request).unwrap(),
            crate::server::RemoteBackendCancelOutcome::OutcomeUnknown { .. }
        ));
        assert!(executor.into_io().calls.is_empty());
    }
}
