use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
mod tls;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PORT: u16 = 48731;
const MAX_IMAGE: u64 = 64 * 1024 * 1024;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
const PAGE: &str = include_str!("page.html");

struct Shared {
    code: [u8; 6],
    dir: PathBuf,
    capture_lock: Mutex<()>,
    attempts: Mutex<Attempts>,
}

struct Attempts {
    failures: u8,
    blocked_until: Option<Instant>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
enum Control {
    Click {
        x: f64,
        y: f64,
        button: Button,
    },
    Drag {
        x: f64,
        y: f64,
        to_x: f64,
        to_y: f64,
    },
    Scroll {
        x: f64,
        y: f64,
        delta: i32,
    },
    Text {
        text: String,
    },
    Key {
        code: String,
        meta: bool,
        ctrl: bool,
        alt: bool,
        shift: bool,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Button {
    Left,
    Right,
}

impl Control {
    fn valid(&self) -> bool {
        fn point(x: f64, y: f64) -> bool {
            x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)
        }
        match self {
            Self::Click { x, y, .. } => point(*x, *y),
            Self::Drag { x, y, to_x, to_y } => point(*x, *y) && point(*to_x, *to_y),
            Self::Scroll { x, y, delta } => point(*x, *y) && (-10..=10).contains(delta),
            Self::Text { text } => !text.is_empty() && text.len() <= 256,
            Self::Key { code, .. } => code.len() <= 24 && mac_key(code).is_some(),
        }
    }
}

fn mac_key(code: &str) -> Option<u16> {
    Some(match code {
        "Enter" => 0x24,
        "Tab" => 0x30,
        "Space" => 0x31,
        "Backspace" => 0x33,
        "Escape" => 0x35,
        "Delete" => 0x75,
        "ArrowLeft" => 0x7b,
        "ArrowRight" => 0x7c,
        "ArrowDown" => 0x7d,
        "ArrowUp" => 0x7e,
        "Home" => 0x73,
        "End" => 0x77,
        "PageUp" => 0x74,
        "PageDown" => 0x79,
        "KeyA" => 0x00,
        "KeyB" => 0x0b,
        "KeyC" => 0x08,
        "KeyD" => 0x02,
        "KeyE" => 0x0e,
        "KeyF" => 0x03,
        "KeyG" => 0x05,
        "KeyH" => 0x04,
        "KeyI" => 0x22,
        "KeyJ" => 0x26,
        "KeyK" => 0x28,
        "KeyL" => 0x25,
        "KeyM" => 0x2e,
        "KeyN" => 0x2d,
        "KeyO" => 0x1f,
        "KeyP" => 0x23,
        "KeyQ" => 0x0c,
        "KeyR" => 0x0f,
        "KeyS" => 0x01,
        "KeyT" => 0x11,
        "KeyU" => 0x20,
        "KeyV" => 0x09,
        "KeyW" => 0x0d,
        "KeyX" => 0x07,
        "KeyY" => 0x10,
        "KeyZ" => 0x06,
        "Digit0" => 0x1d,
        "Digit1" => 0x12,
        "Digit2" => 0x13,
        "Digit3" => 0x14,
        "Digit4" => 0x15,
        "Digit5" => 0x17,
        "Digit6" => 0x16,
        "Digit7" => 0x1a,
        "Digit8" => 0x1c,
        "Digit9" => 0x19,
        _ => return None,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let app_bundle = env::current_exe()?
        .to_string_lossy()
        .contains(".app/Contents/MacOS/");
    let result = run(app_bundle).await;
    if app_bundle {
        if let Err(error) = &result {
            app_alert(&error.to_string());
        }
    }
    result
}

async fn run(app_bundle: bool) -> Result<()> {
    let args: Vec<String> = env::args()
        .skip(1)
        .filter(|arg| !app_bundle || !arg.starts_with("-psn_"))
        .collect();
    let address = match args.as_slice() {
        [] => SocketAddr::new(auto_ip()?, PORT),
        [host, ip] if host == "host" => SocketAddr::new(ip.parse()?, PORT),
        [host, ip, port] if host == "host" => SocketAddr::new(ip.parse()?, port.parse()?),
        _ => return Err("usage: screenlink [host <mac-lan-ip> [port]]".into()),
    };
    if !matches!(address.ip(), IpAddr::V4(ip) if ip.is_private() || ip.is_loopback())
        || address.port() == 0
    {
        return Err("use a private IPv4 address and a nonzero port".into());
    }
    host(address, app_bundle).await
}

fn auto_ip() -> Result<IpAddr> {
    for n in 0..16 {
        let output = Command::new("/usr/sbin/ipconfig")
            .args(["getifaddr", &format!("en{n}")])
            .output()?;
        if let Ok(ip) = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<Ipv4Addr>()
        {
            if ip.is_private() {
                return Ok(IpAddr::V4(ip));
            }
        }
    }
    Err("no private LAN address found; run screenlink host <mac-lan-ip>".into())
}

#[cfg(target_os = "macos")]
async fn host(address: SocketAddr, app_bundle: bool) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let dir = env::var_os("SCREENLINK_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".screenlink")))
        .ok_or("HOME is unset")?;
    if dir.exists() && dir.symlink_metadata()?.file_type().is_symlink() {
        return Err("~/.screenlink must not be a symlink".into());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    let _ = fs::remove_file(dir.join("capture.png"));
    let (tls, fingerprint) = tls::load_or_create(&dir, address.ip()).await?;
    let mut random = [0u8; 4];
    File::open("/dev/urandom")?.read_exact(&mut random)?;
    let secret = format!("{:06}", u32::from_le_bytes(random) % 1_000_000);
    let code: [u8; 6] = secret.as_bytes().try_into()?;
    let app = Router::new()
        .route("/", get(page))
        .route("/app.js", get(script))
        .route("/style.css", get(style))
        .route("/shot", post(shot))
        .route("/control", post(control))
        .with_state(Arc::new(Shared {
            code,
            dir: dir.clone(),
            capture_lock: Mutex::new(()),
            attempts: Mutex::new(Attempts {
                failures: 0,
                blocked_until: None,
            }),
        }));
    let listener = std::net::TcpListener::bind(address)?;
    let url = format!("https://{address}/");
    println!("Open {url} on a device on the same LAN");
    println!("TLS certificate SHA-256: {fingerprint}");
    if !app_bundle {
        println!("Session code: {secret}");
    }
    println!("Grant Screen Recording and turn on Screenlink in Accessibility settings. Ctrl-C stops sharing.");
    if app_bundle {
        let cleanup = dir.clone();
        thread::spawn(move || {
            let result = (|| -> Result<()> {
                let mut dialog = Command::new(dialog_path())
                    .args(["share", &url])
                    .stdin(Stdio::piped())
                    .spawn()?;
                dialog
                    .stdin
                    .take()
                    .ok_or("dialog input unavailable")?
                    .write_all(format!("{secret}\n{fingerprint}").as_bytes())?;
                dialog.wait()?;
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("dialog failed: {error}");
            }
            let _ = fs::remove_file(cleanup.join("capture.png"));
            std::process::exit(0);
        });
    }
    axum_server::from_tcp_rustls(listener, tls)?
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
async fn host(_: SocketAddr, _: bool) -> Result<()> {
    Err("the host requires macOS".into())
}

fn dialog_path() -> PathBuf {
    env::current_exe()
        .unwrap()
        .with_file_name("screenlink-dialog")
}

fn app_alert(message: &str) {
    let _ = Command::new(dialog_path())
        .args(["error", message])
        .status();
}

async fn page() -> Response {
    let mut response = reply(
        StatusCode::OK,
        PAGE.as_bytes().to_vec(),
        "text/html; charset=utf-8",
    );
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src blob:; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'".parse().unwrap());
    response
}

async fn script() -> Response {
    reply(
        StatusCode::OK,
        include_bytes!("app.js").to_vec(),
        "text/javascript; charset=utf-8",
    )
}

async fn style() -> Response {
    reply(
        StatusCode::OK,
        include_bytes!("style.css").to_vec(),
        "text/css; charset=utf-8",
    )
}

fn authorization(state: &Shared, headers: &HeaderMap) -> StatusCode {
    let mut attempts = state.attempts.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(until) = attempts.blocked_until {
        if Instant::now() < until {
            return StatusCode::TOO_MANY_REQUESTS;
        }
        attempts.failures = 0;
        attempts.blocked_until = None;
    }
    let valid = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|value| {
            value.len() == state.code.len() && bool::from(state.code.ct_eq(value.as_bytes()))
        });
    if valid {
        attempts.failures = 0;
        return StatusCode::OK;
    }
    // ponytail: one LAN-wide limit; use per-client limits if this ever needs to serve many users.
    attempts.failures += 1;
    if attempts.failures >= 5 {
        attempts.blocked_until = Some(Instant::now() + Duration::from_secs(60));
        StatusCode::TOO_MANY_REQUESTS
    } else {
        StatusCode::UNAUTHORIZED
    }
}

fn auth_error(status: StatusCode) -> Response {
    if status == StatusCode::TOO_MANY_REQUESTS {
        error(status, "Too many wrong codes. Try again in one minute")
    } else {
        error(status, "Invalid session code")
    }
}

async fn shot(State(state): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    let auth = authorization(&state, &headers);
    if auth != StatusCode::OK {
        return auth_error(auth);
    }
    let result = tokio::task::spawn_blocking(move || {
        let _guard = state.capture_lock.lock().unwrap_or_else(|e| e.into_inner());
        capture_png(&state.dir).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(png)) => reply(StatusCode::OK, png, "image/png"),
        failure => {
            eprintln!("capture failed: {failure:?}");
            error(StatusCode::INTERNAL_SERVER_ERROR, "Screen capture failed. Check Screen Recording permission for Screenlink, then retry")
        }
    }
}

async fn control(State(state): State<Arc<Shared>>, headers: HeaderMap, body: Body) -> Response {
    let auth = authorization(&state, &headers);
    if auth != StatusCode::OK {
        return auth_error(auth);
    }
    let Ok(bytes) = to_bytes(body, 1024).await else {
        return error(StatusCode::BAD_REQUEST, "Invalid control action");
    };
    let Ok(action) = serde_json::from_slice::<Control>(&bytes) else {
        return error(StatusCode::BAD_REQUEST, "Invalid control action");
    };
    if !action.valid() {
        return error(StatusCode::BAD_REQUEST, "Invalid control action");
    }
    #[cfg(target_os = "macos")]
    {
        if !mac::input_allowed() {
            return error(
                StatusCode::FORBIDDEN,
                "Turn on Screenlink in Mac System Settings > Privacy & Security > Accessibility, then retry",
            );
        }
        if let Err(message) = mac::apply(action) {
            return error(StatusCode::INTERNAL_SERVER_ERROR, message);
        }
        reply(StatusCode::NO_CONTENT, Vec::new(), "text/plain")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = action;
        error(StatusCode::NOT_IMPLEMENTED, "Control requires macOS")
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    reply(
        status,
        message.as_bytes().to_vec(),
        "text/plain; charset=utf-8",
    )
}

fn reply(status: StatusCode, body: Vec<u8>, content_type: &'static str) -> Response {
    let mut response = (status, [(header::CONTENT_TYPE, content_type)], body).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    response
}

#[cfg(target_os = "macos")]
fn capture_png(dir: &Path) -> Result<Vec<u8>> {
    let path = dir.join("capture.png");
    let _ = fs::remove_file(&path);
    let mut child = Command::new("/usr/sbin/screencapture")
        .args(["-x", "-C", "-m", "-t", "png", "-T", "0"])
        .arg(&path)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&path);
            return Err("screen capture timed out".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    let png = fs::metadata(&path).and_then(|meta| {
        if meta.len() > MAX_IMAGE {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "screenshot exceeds 64 MiB",
            ))
        } else {
            fs::read(&path)
        }
    });
    let _ = fs::remove_file(&path);
    let png = png?;
    if !status.success() || !png.starts_with(PNG) {
        return Err("screen capture failed".into());
    }
    Ok(png)
}

#[cfg(not(target_os = "macos"))]
fn capture_png(_: &Path) -> Result<Vec<u8>> {
    Err("capture requires macOS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_validation() {
        let parse = |json| serde_json::from_str::<Control>(json).unwrap();
        assert!(parse(r#"{"action":"click","x":0.5,"y":1.0,"button":"left"}"#).valid());
        assert!(!parse(r#"{"action":"click","x":1.1,"y":0.0,"button":"left"}"#).valid());
        assert!(!parse(r#"{"action":"scroll","x":0.5,"y":0.5,"delta":100}"#).valid());
    }

    #[test]
    fn session_code_gate() {
        let state = Shared {
            code: *b"123456",
            dir: PathBuf::new(),
            capture_lock: Mutex::new(()),
            attempts: Mutex::new(Attempts {
                failures: 0,
                blocked_until: None,
            }),
        };
        let mut headers = HeaderMap::new();
        assert_eq!(authorization(&state, &headers), StatusCode::UNAUTHORIZED);
        headers.insert(header::AUTHORIZATION, "Bearer 123457".parse().unwrap());
        assert_eq!(authorization(&state, &headers), StatusCode::UNAUTHORIZED);
        headers.insert(header::AUTHORIZATION, "Bearer 123456".parse().unwrap());
        assert_eq!(authorization(&state, &headers), StatusCode::OK);
        headers.insert(header::AUTHORIZATION, "Bearer 000000".parse().unwrap());
        for _ in 0..4 {
            assert_eq!(authorization(&state, &headers), StatusCode::UNAUTHORIZED);
        }
        assert_eq!(
            authorization(&state, &headers),
            StatusCode::TOO_MANY_REQUESTS
        );
        headers.insert(header::AUTHORIZATION, "Bearer 123456".parse().unwrap());
        assert_eq!(
            authorization(&state, &headers),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
