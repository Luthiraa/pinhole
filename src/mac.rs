use super::{Button, Control};
use std::{ffi::c_void, ptr, thread, time::Duration};

#[repr(C)]
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}
#[repr(C)]
struct Size {
    width: f64,
    height: f64,
}
#[repr(C)]
struct Rect {
    origin: Point,
    size: Size,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> Rect;
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        kind: u32,
        point: Point,
        button: u32,
    ) -> *mut c_void;
    fn CGEventCreateScrollWheelEvent2(
        source: *const c_void,
        units: u32,
        count: u32,
        wheel1: i32,
        wheel2: i32,
        wheel3: i32,
    ) -> *mut c_void;
    fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
    fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, string: *const u16);
    fn CGEventSetFlags(event: *mut c_void, flags: u64);
    fn CGEventSetLocation(event: *mut c_void, point: Point);
    fn CGEventPost(tap: u32, event: *mut c_void);
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(object: *const c_void);
}

struct Event(*mut c_void);
impl Event {
    fn new(ptr: *mut c_void) -> Result<Self, &'static str> {
        if ptr.is_null() {
            Err("Could not create an input event")
        } else {
            Ok(Self(ptr))
        }
    }
    fn post(&self) {
        unsafe { CGEventPost(0, self.0) }
    }
}
impl Drop for Event {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}

pub fn input_allowed() -> bool {
    unsafe { CGPreflightPostEventAccess() }
}

fn point(x: f64, y: f64) -> Point {
    let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    Point {
        x: bounds.origin.x + x * bounds.size.width,
        y: bounds.origin.y + y * bounds.size.height,
    }
}

fn mouse(kind: u32, at: Point, button: u32) -> Result<(), &'static str> {
    Event::new(unsafe { CGEventCreateMouseEvent(ptr::null(), kind, at, button) })?.post();
    Ok(())
}

fn key(code: u16, flags: u64, text: Option<&[u16]>) -> Result<(), &'static str> {
    for down in [true, false] {
        let event = Event::new(unsafe { CGEventCreateKeyboardEvent(ptr::null(), code, down) })?;
        unsafe {
            CGEventSetFlags(event.0, flags);
            if let Some(text) = text {
                CGEventKeyboardSetUnicodeString(event.0, text.len(), text.as_ptr());
            }
        }
        event.post();
    }
    Ok(())
}

pub fn apply(action: Control) -> Result<(), &'static str> {
    match action {
        Control::Click { x, y, button } => {
            let at = point(x, y);
            let (down, up, number) = match button {
                Button::Left => (1, 2, 0),
                Button::Right => (3, 4, 1),
            };
            mouse(down, at, number)?;
            mouse(up, at, number)
        }
        Control::Drag { x, y, to_x, to_y } => {
            let start = point(x, y);
            let end = point(to_x, to_y);
            mouse(1, start, 0)?;
            for step in 1..=8 {
                let t = step as f64 / 8.0;
                mouse(
                    6,
                    Point {
                        x: start.x + (end.x - start.x) * t,
                        y: start.y + (end.y - start.y) * t,
                    },
                    0,
                )?;
                thread::sleep(Duration::from_millis(12));
            }
            mouse(2, end, 0)
        }
        Control::Scroll { x, y, delta } => {
            let event = Event::new(unsafe {
                CGEventCreateScrollWheelEvent2(ptr::null(), 0, 1, delta, 0, 0)
            })?;
            unsafe {
                CGEventSetLocation(event.0, point(x, y));
            }
            event.post();
            Ok(())
        }
        Control::Text { text } => {
            for character in text.chars() {
                let mut buffer = [0u16; 2];
                key(0, 0, Some(character.encode_utf16(&mut buffer)))?;
            }
            Ok(())
        }
        Control::Key {
            code,
            meta,
            ctrl,
            alt,
            shift,
        } => {
            let flags = (u64::from(shift) << 17)
                | (u64::from(ctrl) << 18)
                | (u64::from(alt) << 19)
                | (u64::from(meta) << 20);
            key(super::mac_key(&code).ok_or("Unknown key")?, flags, None)
        }
    }
}
