use super::{check_code, reply};
use crate::tiles::{self, DiffState, DirtyTile};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::Response;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::sync::Notify;

#[cfg(target_os = "macos")]
use std::ffi::{c_char, c_void, CStr};

const VIEW: &str = include_str!("view.html");

pub(super) struct Hub {
    inner: Mutex<Inner>,
    notify: Notify,
}

struct Inner {
    width: u16,
    height: u16,
    epoch: u32,
    next_gen: u64,
    tiles: Vec<Slot>,
    watchers: usize,
    capture: Option<Capture>,
    error: Option<String>,
}

struct Slot {
    x: u16,
    y: u16,
    gen: u64,
    jpeg: Vec<u8>,
}

struct EncodedTile {
    x: u16,
    y: u16,
    jpeg: Vec<u8>,
}

struct Outbound {
    epoch: u32,
    watermark: u64,
    messages: Vec<Vec<u8>>,
}

#[derive(Default)]
struct Subscriber {
    epoch: u32,
    watermark: u64,
}

struct Watch {
    hub: Arc<Hub>,
}

struct Job {
    width: u16,
    height: u16,
    hashes: Vec<Option<u64>>,
    dirty: Vec<DirtyTile>,
}

struct Queue {
    stop: bool,
    job: Option<Job>,
}

struct CaptureState {
    hub: Arc<Hub>,
    diff: Mutex<DiffState>,
    queue: Mutex<Queue>,
    cv: Condvar,
}

#[cfg(target_os = "macos")]
struct Capture {
    raw: *mut c_void,
    state: *mut CaptureState,
    worker: Option<JoinHandle<()>>,
}

#[cfg(target_os = "macos")]
unsafe impl Send for Capture {}

#[cfg(target_os = "macos")]
struct SendPtr(*mut CaptureState);

#[cfg(target_os = "macos")]
unsafe impl Send for SendPtr {}

#[cfg(not(target_os = "macos"))]
struct Capture;

#[cfg(target_os = "macos")]
extern "C" {
    fn pinhole_capture_start(
        ctx: *mut c_void,
        on_frame: extern "C" fn(*mut c_void, *const u8, u32, u32, u32),
        on_error: extern "C" fn(*mut c_void, *const c_char),
    ) -> *mut c_void;
    fn pinhole_capture_stop(raw: *mut c_void);
    fn pinhole_jpeg(
        bgra: *const u8,
        width: u32,
        height: u32,
        out_bytes: *mut *mut u8,
        out_len: *mut u32,
    ) -> i32;
    fn pinhole_free(bytes: *mut u8);
}

impl Hub {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                width: 0,
                height: 0,
                epoch: 0,
                next_gen: 0,
                tiles: Vec::new(),
                watchers: 0,
                capture: None,
                error: None,
            }),
            notify: Notify::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn watch(self: &Arc<Self>) -> Watch {
        let start = {
            let mut inner = self.lock();
            inner.watchers += 1;
            inner.watchers == 1 && inner.capture.is_none()
        };
        if start {
            start_capture(self);
        }
        Watch {
            hub: Arc::clone(self),
        }
    }

    fn fail(&self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("capture failed: {message}");
        self.lock().error = Some(message);
        self.notify.notify_waiters();
    }

    fn error(&self) -> Option<String> {
        self.lock().error.clone()
    }

    fn publish(&self, width: u16, height: u16, tiles: Vec<EncodedTile>) {
        {
            let mut inner = self.lock();
            inner.error = None;
            if inner.width != width || inner.height != height {
                inner.epoch = inner.epoch.wrapping_add(1).max(1);
                inner.width = width;
                inner.height = height;
                let count = (width as usize).div_ceil(tiles::TILE)
                    * (height as usize).div_ceil(tiles::TILE);
                inner.tiles.clear();
                inner.tiles.resize_with(count, Slot::empty);
            }
            let cols = (width as usize).div_ceil(tiles::TILE);
            for tile in tiles {
                let index = (tile.y as usize / tiles::TILE) * cols + tile.x as usize / tiles::TILE;
                if index >= inner.tiles.len() {
                    continue;
                }
                inner.next_gen = inner.next_gen.wrapping_add(1).max(1);
                inner.tiles[index] = Slot {
                    x: tile.x,
                    y: tile.y,
                    gen: inner.next_gen,
                    jpeg: tile.jpeg,
                };
            }
        }
        self.notify.notify_waiters();
    }

    fn pending(&self, subscriber: &Subscriber) -> Outbound {
        let inner = self.lock();
        if inner.epoch == 0 || inner.width == 0 {
            return Outbound {
                epoch: subscriber.epoch,
                watermark: subscriber.watermark,
                messages: Vec::new(),
            };
        }
        let mut messages = Vec::new();
        let mut watermark = subscriber.watermark;
        if subscriber.epoch != inner.epoch {
            messages.push(tiles::geometry_message(inner.width, inner.height).to_vec());
            watermark = 0;
        }
        let mut max_gen = watermark;
        for slot in &inner.tiles {
            if slot.gen > watermark {
                messages.push(tiles::tile_message(slot.x, slot.y, &slot.jpeg));
                max_gen = max_gen.max(slot.gen);
            }
        }
        Outbound {
            epoch: inner.epoch,
            watermark: max_gen,
            messages,
        }
    }
}

impl Slot {
    fn empty() -> Self {
        Self {
            x: 0,
            y: 0,
            gen: 0,
            jpeg: Vec::new(),
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let capture = {
            let mut inner = self.hub.lock();
            inner.watchers = inner.watchers.saturating_sub(1);
            if inner.watchers == 0 {
                inner.capture.take()
            } else {
                None
            }
        };
        let stopped = capture.is_some();
        drop(capture);
        if !stopped {
            return;
        }
        let mut inner = self.hub.lock();
        if inner.watchers == 0 {
            inner.tiles.clear();
            inner.width = 0;
            inner.height = 0;
            inner.epoch = inner.epoch.wrapping_add(1);
            inner.error = None;
        }
    }
}

fn start_capture(hub: &Arc<Hub>) {
    #[cfg(not(target_os = "macos"))]
    {
        hub.fail("capture requires macOS");
    }
    #[cfg(target_os = "macos")]
    {
        if !super::mac::capture_allowed() {
            hub.fail("Allow Pinhole in System Settings > Privacy & Security > Screen Recording, then restart pinhole");
            return;
        }
        match Capture::start(Arc::clone(hub)) {
            Ok(capture) => {
                let mut inner = hub.lock();
                if inner.watchers == 0 {
                    drop(inner);
                    drop(capture);
                } else {
                    inner.capture = Some(capture);
                }
            }
            Err(message) => hub.fail(message),
        }
    }
}

#[cfg(target_os = "macos")]
impl Capture {
    fn start(hub: Arc<Hub>) -> Result<Self, String> {
        let state = Box::into_raw(Box::new(CaptureState {
            hub,
            diff: Mutex::new(DiffState::default()),
            queue: Mutex::new(Queue {
                stop: false,
                job: None,
            }),
            cv: Condvar::new(),
        }));
        let worker_state = SendPtr(state);
        let worker = thread::Builder::new()
            .name("pinhole-tiles".into())
            .spawn(move || worker(worker_state))
            .map_err(|error| {
                unsafe { drop(Box::from_raw(state)) };
                error.to_string()
            })?;
        let raw = unsafe { pinhole_capture_start(state.cast(), on_frame, on_error) };
        if raw.is_null() {
            if let Some(state) = unsafe { state.as_ref() } {
                let mut queue = state
                    .queue
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                queue.stop = true;
                state.cv.notify_one();
            }
            let _ = worker.join();
            unsafe { drop(Box::from_raw(state)) };
            return Err("could not start screen capture".into());
        }
        Ok(Self {
            raw,
            state,
            worker: Some(worker),
        })
    }
}

#[cfg(target_os = "macos")]
impl Drop for Capture {
    fn drop(&mut self) {
        if let Some(state) = unsafe { self.state.as_ref() } {
            let mut queue = state
                .queue
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            queue.stop = true;
            state.cv.notify_one();
        }
        if !self.raw.is_null() {
            unsafe { pinhole_capture_stop(self.raw) };
            self.raw = std::ptr::null_mut();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if !self.state.is_null() {
            unsafe { drop(Box::from_raw(self.state)) };
            self.state = std::ptr::null_mut();
        }
    }
}

#[cfg(target_os = "macos")]
fn worker(state: SendPtr) {
    let state = unsafe { &*state.0 };
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    loop {
        if queue.stop {
            return;
        }
        let Some(job) = queue.job.take() else {
            queue = state
                .cv
                .wait(queue)
                .unwrap_or_else(|error| error.into_inner());
            continue;
        };
        drop(queue);
        // ponytail: one worker, and the queue keeps only the newest frame.
        publish_job(state, job);
        queue = state
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
    }
}

#[cfg(target_os = "macos")]
fn publish_job(state: &CaptureState, mut job: Job) {
    let mut encoded = Vec::new();
    for tile in job.dirty {
        match encode_jpeg(&tile.pixels, tile.w, tile.h) {
            Some(jpeg) => encoded.push(EncodedTile {
                x: tile.x,
                y: tile.y,
                jpeg,
            }),
            None => job.hashes[tile.index] = tile.previous,
        }
    }
    {
        let mut diff = state.diff.lock().unwrap_or_else(|error| error.into_inner());
        diff.commit(job.width, job.height, job.hashes);
    }
    if !encoded.is_empty() {
        state.hub.publish(job.width, job.height, encoded);
    }
}

#[cfg(target_os = "macos")]
fn encode_jpeg(pixels: &[u8], width: u16, height: u16) -> Option<Vec<u8>> {
    if pixels.len() != width as usize * height as usize * 4 {
        return None;
    }
    let mut ptr = std::ptr::null_mut();
    let mut len = 0u32;
    let rc = unsafe {
        pinhole_jpeg(
            pixels.as_ptr(),
            u32::from(width),
            u32::from(height),
            &mut ptr,
            &mut len,
        )
    };
    if rc != 0 || ptr.is_null() || len == 0 {
        if !ptr.is_null() {
            unsafe { pinhole_free(ptr) };
        }
        return None;
    }
    let jpeg = unsafe { std::slice::from_raw_parts(ptr, len as usize) }.to_vec();
    unsafe { pinhole_free(ptr) };
    Some(jpeg)
}

#[cfg(target_os = "macos")]
extern "C" fn on_error(ctx: *mut c_void, message: *const c_char) {
    if ctx.is_null() {
        return;
    }
    let message = if message.is_null() {
        "screen capture failed".to_string()
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    };
    unsafe { &*ctx.cast::<CaptureState>() }.hub.fail(message);
}

#[cfg(target_os = "macos")]
extern "C" fn on_frame(ctx: *mut c_void, bgra: *const u8, width: u32, height: u32, stride: u32) {
    if ctx.is_null() || bgra.is_null() {
        return;
    }
    let state = unsafe { &*ctx.cast::<CaptureState>() };
    let bytes = (height as usize).saturating_mul(stride as usize);
    let frame = unsafe { std::slice::from_raw_parts(bgra, bytes) };
    let plan = {
        let diff = state.diff.lock().unwrap_or_else(|error| error.into_inner());
        match diff.plan(frame, width, height, stride as usize) {
            Ok(plan) if !plan.dirty.is_empty() => plan,
            _ => return,
        }
    };
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if queue.stop {
        return;
    }
    queue.job = Some(Job {
        width: plan.width,
        height: plan.height,
        hashes: plan.hashes,
        dirty: plan.dirty,
    });
    state.cv.notify_one();
}

pub(super) async fn view_page() -> Response {
    let mut response = reply(StatusCode::OK, VIEW, "text/html; charset=utf-8");
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
            .parse()
            .unwrap(),
    );
    response
}

pub(super) async fn socket(
    ws: WebSocketUpgrade,
    State(state): State<Arc<super::Shared>>,
) -> Response {
    ws.max_message_size(64)
        .max_frame_size(64)
        .on_upgrade(move |socket| session(socket, state))
}

async fn session(mut socket: WebSocket, state: Arc<super::Shared>) {
    let first = tokio::time::timeout(Duration::from_secs(5), socket.recv()).await;
    let Ok(Some(Ok(Message::Text(text)))) = first else {
        return;
    };
    let status = check_code(&state, text.trim());
    if status != StatusCode::OK {
        let message = if status == StatusCode::TOO_MANY_REQUESTS {
            "Too many wrong codes. Try again in one minute"
        } else {
            "Invalid session code"
        };
        let _ = socket.send(Message::Text(message.into())).await;
        return;
    }
    let _watch = state.hub.watch();
    let mut subscriber = Subscriber::default();
    let mut reported = None;
    loop {
        let notified = state.hub.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if let Some(message) = state.hub.error() {
            if reported.as_ref() != Some(&message)
                && socket
                    .send(Message::Text(message.clone().into()))
                    .await
                    .is_err()
            {
                break;
            }
            reported = Some(message);
        }
        let pending = state.hub.pending(&subscriber);
        if pending.messages.is_empty() {
            tokio::select! {
                _ = notified => continue,
                incoming = socket.recv() => {
                    if !matches!(incoming, Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_)))) {
                        break;
                    }
                }
            }
            continue;
        }
        let mut sent = true;
        for message in pending.messages {
            if socket.send(Message::Binary(message.into())).await.is_err() {
                sent = false;
                break;
            }
        }
        if !sent {
            break;
        }
        subscriber.epoch = pending.epoch;
        subscriber.watermark = pending.watermark;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clients_receive_a_changed_tile_once() {
        let hub = Hub::new();
        hub.publish(128, 64, vec![tile(0, 0, vec![0xff, 0xd8])]);
        let mut subscriber = Subscriber::default();
        let first = hub.pending(&subscriber);
        assert_eq!(first.messages.len(), 2);
        assert_eq!(first.messages[0][0], 1);
        assert_eq!(first.messages[1][0], 2);
        subscriber.epoch = first.epoch;
        subscriber.watermark = first.watermark;
        assert!(hub.pending(&subscriber).messages.is_empty());
        hub.publish(128, 64, vec![tile(0, 0, vec![1])]);
        let next = hub.pending(&subscriber);
        assert_eq!(next.messages.len(), 1);
        assert_eq!(next.messages[0][0], 2);
        subscriber.epoch = next.epoch;
        subscriber.watermark = next.watermark;
        hub.publish(64, 64, vec![tile(0, 0, vec![2])]);
        let resized = hub.pending(&subscriber);
        assert_eq!(resized.messages[0][0], 1);
        assert!(resized
            .messages
            .iter()
            .skip(1)
            .all(|message| message[0] == 2));
    }

    fn tile(x: u16, y: u16, jpeg: Vec<u8>) -> EncodedTile {
        EncodedTile { x, y, jpeg }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn jpeg_tiles_are_jpegs() {
        let red = vec![0u8, 0, 255, 255].repeat(8 * 8);
        let green = vec![0u8, 255, 0, 255].repeat(8 * 8);
        let red_jpeg = encode_jpeg(&red, 8, 8).unwrap();
        let green_jpeg = encode_jpeg(&green, 8, 8).unwrap();
        assert!(red_jpeg.starts_with(&[0xff, 0xd8, 0xff]));
        assert_ne!(red_jpeg, green_jpeg);
    }
}
