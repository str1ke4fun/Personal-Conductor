use std::{
    fs::OpenOptions,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const DEV_SERVER_ADDR: &str = "127.0.0.1:1420";
const DEV_SERVER_TIMEOUT: Duration = Duration::from_secs(20);

pub struct DevServerGuard {
    child: Option<Child>,
}

impl Drop for DevServerGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[cfg(windows)]
            kill_process_tree(child.id());

            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn ensure_vite_dev_server() -> Option<DevServerGuard> {
    if std::env::var_os("CONDUCTOR_SKIP_DEV_SERVER").is_some() || is_dev_server_ready() {
        return None;
    }

    let desktop_dir = match find_desktop_dir() {
        Some(path) => path,
        None => {
            eprintln!("failed to locate apps/desktop for Vite dev server autostart");
            return None;
        }
    };

    let child = match spawn_vite(&desktop_dir) {
        Ok(child) => child,
        Err(err) => {
            eprintln!(
                "failed to start Vite dev server in {}: {err}",
                desktop_dir.display()
            );
            return None;
        }
    };

    if !wait_for_dev_server(DEV_SERVER_TIMEOUT) {
        eprintln!("Vite dev server did not become ready at {DEV_SERVER_ADDR}");
    }

    Some(DevServerGuard { child: Some(child) })
}

fn is_dev_server_ready() -> bool {
    let Ok(addr) = DEV_SERVER_ADDR.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

fn wait_for_dev_server(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if is_dev_server_ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

fn find_desktop_dir() -> Option<PathBuf> {
    if let Ok(current_dir) = std::env::current_dir() {
        let candidate = current_dir.join("apps").join("desktop");
        if is_desktop_dir(&candidate) {
            return Some(candidate);
        }
    }

    let exe_path = std::env::current_exe().ok()?;
    for ancestor in exe_path.ancestors() {
        let candidate = ancestor.join("apps").join("desktop");
        if is_desktop_dir(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn is_desktop_dir(path: &Path) -> bool {
    path.join("package.json").is_file() && path.join("vite.config.ts").is_file()
}

fn spawn_vite(desktop_dir: &Path) -> std::io::Result<Child> {
    let mut command = Command::new(npm_command());
    command
        .args(["run", "dev"])
        .current_dir(desktop_dir)
        .stdin(Stdio::null())
        .stdout(log_stdio(&desktop_dir.join(".tauri-direct-dev.out.log")))
        .stderr(log_stdio(&desktop_dir.join(".tauri-direct-dev.err.log")));

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn()
}

fn npm_command() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn log_stdio(path: &Path) -> Stdio {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null())
}
