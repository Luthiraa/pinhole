use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
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
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
#[cfg(target_os = "macos")]
use tokio::io::AsyncReadExt;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
mod session;
mod stream;
mod tiles;
#[cfg(target_os = "macos")]
mod tls;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CLIENT_ORIGIN: &str = "https://pinhole-client.vercel.app";
const PORT: u16 = 48731;
const MAX_IMAGE: u64 = 64 * 1024 * 1024;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
const HELP: &str = "Pinhole — private Mac control from your terminal

Usage: pinhole [command]

  pinhole                       Start sharing on your local network
  pinhole host <ip>              Choose a local address (port: 48731)
  pinhole start [ip]             Start a manual background host
  pinhole status                Show background host status and connection code
  pinhole stop                  Stop the background host
  pinhole permissions           Request Screen Recording and Accessibility
  pinhole --version             Print the version
  pinhole --help                Show this help

Open the HTTPS host address, compare the certificate fingerprint, then enter the session code.
Keep this terminal open. Ctrl-C stops sharing.";

struct Shared {
    code: [u8; 6],
    attempts: Mutex<Attempts>,
    hub: Arc<stream::Hub>,
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

fn main() -> Result<()> {
    #[cfg(target_os = "macos")]
    if env::args().nth(1).as_deref() == Some("--session") {
        return mac::run_app();
    }
    run()
}

#[tokio::main]
async fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if let [command] = args.as_slice() {
        match command.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!("pinhole {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            _ => {}
        }
    }
    #[cfg(target_os = "macos")]
    if let [command, dir] = args.as_slice() {
        if command == "--session" {
            let dir = Path::new(dir);
            let (start, connection) = session::connect(dir)?;
            let result = if start.args == ["permissions"] {
                permissions(connection, dir).await
            } else {
                match host_address(&start.args) {
                    Ok(address) => {
                        host(address, start.state_dir, connection, start.background).await
                    }
                    Err(error) => Err(error),
                }
            };
            session::finish(dir, result.is_ok())?;
            return result;
        }
    }
    Err("use the npm pinhole command to launch the background app".into())
}

fn host_address(args: &[String]) -> Result<SocketAddr> {
    let address = match args {
        [] => SocketAddr::new(auto_ip()?, PORT),
        [host, ip] if host == "host" => SocketAddr::new(ip.parse()?, PORT),
        _ => return Err("invalid command; run pinhole --help".into()),
    };
    if !matches!(address.ip(), IpAddr::V4(ip) if ip.is_private() || ip.is_loopback()) {
        return Err("use a private IPv4 address".into());
    }
    Ok(address)
}

#[cfg(target_os = "macos")]
async fn permissions(mut connection: tokio::net::UnixStream, dir: &Path) -> Result<()> {
    let (mut screen, control) = mac::request_permissions();
    if !screen {
        // Exercise the capture service so macOS can present its recording prompt.
        // The probe uses the private CLI session directory and deletes its image.
        let _ = capture_png(dir);
        screen = mac::capture_allowed();
    }
    println!(
        "Screen Recording: {}",
        if screen { "allowed" } else { "not yet allowed" }
    );
    println!(
        "Accessibility: {}",
        if control {
            "allowed"
        } else {
            "not yet allowed"
        }
    );
    if !screen || !control {
        println!("Enable Pinhole in System Settings > Privacy & Security.");
        println!(
            "Keep this command open while enabling permissions, then press Ctrl-C and run pinhole."
        );
        let pane = if !screen {
            "ScreenCapture"
        } else {
            "Accessibility"
        };
        Command::new("/usr/bin/open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_{pane}"
            ))
            .status()?;
        // Keep the native app alive so macOS can present and register the requests.
        let _ = connection.read_u8().await;
    }
    Ok(())
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
    Err("no private LAN address found; run pinhole host <mac-lan-ip>".into())
}

#[cfg(target_os = "macos")]
async fn host(
    address: SocketAddr,
    state_dir: Option<PathBuf>,
    mut connection: tokio::net::UnixStream,
    background: bool,
) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let dir = state_dir
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".pinhole")))
        .ok_or("HOME is unset")?;
    if dir.exists() && dir.symlink_metadata()?.file_type().is_symlink() {
        return Err("~/.pinhole must not be a symlink".into());
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
        .route("/", get(stream::view_page))
        .route("/stream", get(stream::socket))
        .route("/control", post(control))
        .layer(middleware::from_fn(client_access))
        .with_state(Arc::new(Shared {
            code,
            attempts: Mutex::new(Attempts {
                failures: 0,
                blocked_until: None,
            }),
            hub: stream::Hub::new(),
        }));
    let listener = std::net::TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    let url = format!("https://{address}/");
    println!("Pinhole {}\n", env!("CARGO_PKG_VERSION"));
    println!("Open  {url}");
    println!("Code  {secret}\n");
    println!("Certificate SHA-256: {fingerprint}");
    println!("Compare this fingerprint before accepting the browser's certificate warning.");
    if !mac::capture_allowed() || !mac::input_allowed() {
        println!("\nPermissions are missing. Stop sharing, run pinhole permissions, then start pinhole again.");
    }
    if background {
        println!("\nBackground host ready. Run pinhole stop to stop sharing.");
    } else {
        println!("\nKeep this terminal open. Ctrl-C stops sharing.");
    }
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    let stop = tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = connection.read_u8() => {},
        }
        shutdown.graceful_shutdown(Some(Duration::from_secs(1)));
        let _ = writeln!(io::stdout(), "\nStopping sharing…");
    });
    let result = axum_server::from_tcp_rustls(listener, tls)?
        .handle(handle)
        .serve(app.into_make_service())
        .await;
    stop.abort();
    let _ = fs::remove_file(dir.join("capture.png"));
    result?;
    Ok(())
}

fn allowed_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    if origin.as_bytes() == CLIENT_ORIGIN.as_bytes() {
        return true;
    }
    let (Some(origin), Some(host)) = (
        origin.to_str().ok(),
        headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok()),
    ) else {
        return false;
    };
    origin
        .strip_prefix("https://")
        .is_some_and(|rest| rest == host)
}

async fn client_access(request: Request, next: Next) -> Response {
    if !allowed_origin(request.headers()) {
        return error(StatusCode::FORBIDDEN, "This origin is not allowed");
    }
    let vercel = request
        .headers()
        .get(header::ORIGIN)
        .is_some_and(|origin| origin.as_bytes() == CLIENT_ORIGIN.as_bytes());
    let mut response = if request.method() == axum::http::Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(header::VARY, "Origin".parse().unwrap());
    if vercel {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            CLIENT_ORIGIN.parse().unwrap(),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            "POST, OPTIONS".parse().unwrap(),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "Authorization, Content-Type".parse().unwrap(),
        );
        headers.insert(
            "access-control-allow-private-network",
            "true".parse().unwrap(),
        );
    }
    response
}

fn authorization(state: &Shared, headers: &HeaderMap) -> StatusCode {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    check_code(state, value)
}

fn check_code(state: &Shared, value: &str) -> StatusCode {
    let valid = value.len() == state.code.len() && bool::from(state.code.ct_eq(value.as_bytes()));
    if valid {
        return StatusCode::OK;
    }
    let mut attempts = state.attempts.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(until) = attempts.blocked_until {
        if Instant::now() < until {
            return StatusCode::TOO_MANY_REQUESTS;
        }
        attempts.failures = 0;
        attempts.blocked_until = None;
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

async fn control(State(state): State<Arc<Shared>>, headers: HeaderMap, body: Body) -> Response {
    let auth = authorization(&state, &headers);
    if auth != StatusCode::OK {
        return auth_error(auth);
    }
    let bytes = match tokio::time::timeout(Duration::from_secs(5), to_bytes(body, 1024)).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return error(StatusCode::BAD_REQUEST, "Invalid control action"),
        Err(_) => return error(StatusCode::REQUEST_TIMEOUT, "Control request timed out"),
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
                "Allow Pinhole in Mac System Settings > Privacy & Security > Accessibility, then restart pinhole",
            );
        }
        if let Err(message) = mac::apply(action) {
            return error(StatusCode::INTERNAL_SERVER_ERROR, message);
        }
        reply(StatusCode::NO_CONTENT, "", "text/plain")
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = action;
        error(StatusCode::NOT_IMPLEMENTED, "Control requires macOS")
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    reply(status, message, "text/plain; charset=utf-8")
}

fn reply(status: StatusCode, body: impl Into<Body>, content_type: &'static str) -> Response {
    let mut response =
        (status, [(header::CONTENT_TYPE, content_type)], body.into()).into_response();
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

    fn state() -> Arc<Shared> {
        Arc::new(Shared {
            code: *b"123456",
            attempts: Mutex::new(Attempts {
                failures: 0,
                blocked_until: None,
            }),
            hub: stream::Hub::new(),
        })
    }

    #[test]
    fn wrong_code_lockout_preserves_authenticated_access() {
        let state = state();
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer 000000".parse().unwrap());
        for _ in 0..4 {
            assert_eq!(authorization(&state, &headers), StatusCode::UNAUTHORIZED);
        }
        assert_eq!(
            authorization(&state, &headers),
            StatusCode::TOO_MANY_REQUESTS
        );
        headers.insert(header::AUTHORIZATION, "Bearer 123456".parse().unwrap());
        assert_eq!(authorization(&state, &headers), StatusCode::OK);
        headers.insert(header::AUTHORIZATION, "Bearer 000000".parse().unwrap());
        assert_eq!(
            authorization(&state, &headers),
            StatusCode::TOO_MANY_REQUESTS
        );
        state.attempts.lock().unwrap().blocked_until =
            Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(authorization(&state, &headers), StatusCode::UNAUTHORIZED);
        assert_eq!(state.attempts.lock().unwrap().failures, 1);
    }

    #[tokio::test]
    async fn control_rejects_unauthenticated_oversized_and_invalid_requests() {
        let state = state();
        assert_eq!(
            control(State(state.clone()), HeaderMap::new(), Body::empty())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer 123456".parse().unwrap());
        for body in [
            vec![b'x'; 1025],
            br#"{"action":"click","x":2,"y":0,"button":"left"}"#.to_vec(),
        ] {
            assert_eq!(
                control(State(state.clone()), headers.clone(), Body::from(body))
                    .await
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
    }

    #[test]
    fn host_port_is_fixed() {
        let args = ["host".into(), "192.168.1.10".into()];
        assert_eq!(host_address(&args).unwrap().port(), PORT);
        assert!(host_address(&["host".into(), "192.168.1.10".into(), "1234".into()]).is_err());
        assert!(host_address(&["host".into(), "8.8.8.8".into()]).is_err());
    }

    #[test]
    fn only_private_client_origin_is_allowed() {
        let mut headers = HeaderMap::new();
        assert!(allowed_origin(&headers));
        headers.insert(header::ORIGIN, CLIENT_ORIGIN.parse().unwrap());
        assert!(allowed_origin(&headers));
        for origin in [
            "https://evil.example",
            "null",
            "https://pinhole-client.vercel.app.evil.example",
        ] {
            headers.insert(header::ORIGIN, origin.parse().unwrap());
            assert!(!allowed_origin(&headers));
        }
        headers.insert(header::HOST, "127.0.0.1:48731".parse().unwrap());
        headers.insert(header::ORIGIN, "https://127.0.0.1:48731".parse().unwrap());
        assert!(allowed_origin(&headers));
        headers.insert(header::ORIGIN, "http://127.0.0.1:48731".parse().unwrap());
        assert!(!allowed_origin(&headers));
    }
}
