//! Local transcription with faster-whisper (CTranslate2, CPU int8).
//!
//! faster-whisper is a Python library, so the app keeps a private virtualenv in
//! its local data dir and runs a long-lived worker (`resources/fw_server.py`)
//! that holds the loaded models in memory. The venv is created and the package
//! installed on first use; models come from the shared Hugging Face cache.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;
use tauri::{AppHandle, Manager};

/// Model ids in the settings / dropdown use this prefix (e.g. `local:medium`).
pub const PREFIX: &str = "local:";

const SERVER_SRC: &str = include_str!("../resources/fw_server.py");

/// The running worker, shared by every transcription (one request at a time).
static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default)]
    text: String,
    #[serde(default)]
    error: String,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// `local:medium` → `Some("medium")`.
pub fn model_name(id: &str) -> Option<&str> {
    id.strip_prefix(PREFIX)
}

/// Whether the private venv still has to be created / populated.
pub fn needs_setup(app: &AppHandle) -> bool {
    venv_dir(app).map(|d| !d.join(".ready").exists()).unwrap_or(true)
}

/// Load a model ahead of time so the first dictation doesn't pay for it.
pub fn warm(app: &AppHandle, model: &str) -> Result<()> {
    request(app, json!({ "cmd": "load", "model": model })).map(|_| ())
}

/// Transcribe a WAV buffer with the given faster-whisper model (blocking).
pub fn transcribe(app: &AppHandle, model: &str, wav: &[u8]) -> Result<String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("whisperaround-{stamp}.wav"));
    std::fs::write(&path, wav).context("escribiendo WAV temporal")?;
    let result = request(
        app,
        json!({ "cmd": "transcribe", "model": model, "path": path.to_string_lossy() }),
    );
    let _ = std::fs::remove_file(&path);
    result
}

/// Send one request to the worker, (re)starting it if needed.
fn request(app: &AppHandle, req: serde_json::Value) -> Result<String> {
    let mut guard = WORKER.lock().unwrap();
    if guard.is_none() {
        *guard = Some(spawn_worker(app)?);
    }
    let worker = guard.as_mut().unwrap();

    let io = (|| -> Result<Reply> {
        writeln!(worker.stdin, "{req}")?;
        worker.stdin.flush()?;
        let mut line = String::new();
        if worker.stdout.read_line(&mut line)? == 0 {
            return Err(anyhow!("el proceso de faster-whisper terminó inesperadamente"));
        }
        Ok(serde_json::from_str(&line)?)
    })();

    match io {
        Ok(r) if r.ok => Ok(r.text),
        Ok(r) => Err(anyhow!("faster-whisper: {}", r.error)),
        Err(e) => {
            // Broken pipe / crash: drop it so the next request starts fresh.
            *guard = None;
            Err(e)
        }
    }
}

fn spawn_worker(app: &AppHandle) -> Result<Worker> {
    let python = ensure_venv(app)?;
    let script = venv_dir(app)?.join("fw_server.py");
    std::fs::write(&script, SERVER_SRC).context("escribiendo fw_server.py")?;

    let mut child = command(&python)
        .arg("-u")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("iniciando faster-whisper")?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow!("sin stdin"))?;
    let stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow!("sin stdout"))?);
    Ok(Worker { child, stdin, stdout })
}

fn venv_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow!("resolviendo carpeta de datos: {e}"))?;
    Ok(dir.join("fw-venv"))
}

fn venv_python(venv: &Path) -> PathBuf {
    if cfg!(windows) {
        venv.join("Scripts").join("python.exe")
    } else {
        venv.join("bin").join("python")
    }
}

/// Make sure the private venv exists with faster-whisper installed and return
/// its interpreter. Slow only the first time (creates venv + pip install).
fn ensure_venv(app: &AppHandle) -> Result<PathBuf> {
    let venv = venv_dir(app)?;
    let python = venv_python(&venv);
    let ready = venv.join(".ready");
    if ready.exists() && python.exists() {
        return Ok(python);
    }

    if !python.exists() {
        std::fs::create_dir_all(&venv)?;
        create_venv(&venv)?;
    }
    run(
        command(&python).args(["-m", "pip", "install", "--disable-pip-version-check", "faster-whisper"]),
        "pip install faster-whisper",
    )?;
    std::fs::write(&ready, "")?;
    Ok(python)
}

#[cfg(windows)]
const PYTHON_HINT: &str = "instálalo desde la Microsoft Store (bandeja › Bienvenida)";
#[cfg(not(windows))]
const PYTHON_HINT: &str = "instálalo desde python.org o con `brew install python` (bandeja › Bienvenida)";

/// Where Python usually lives on macOS (Homebrew on Apple Silicon / Intel,
/// the python.org installer). Apps opened from Finder get a bare
/// `/usr/bin:/bin:/usr/sbin:/sbin` PATH that misses all of them, leaving only
/// the old Command Line Tools Python (or a prompt to install them).
#[cfg(target_os = "macos")]
const MACOS_PYTHON_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/Library/Frameworks/Python.framework/Versions/Current/bin",
];

/// System Python launchers to try, in order of preference.
fn python_candidates() -> &'static [(&'static str, &'static [&'static str])] {
    if cfg!(windows) {
        &[("py", &["-3"]), ("python", &[]), ("python3", &[])]
    } else {
        &[("python3", &[]), ("python", &[])]
    }
}

/// Version string (e.g. `"Python 3.12.7"`) of the first working system
/// Python 3, or `None`. On Windows the Microsoft Store `python.exe` alias that
/// exists before Python is installed exits with an error, so it counts as
/// missing.
pub fn detect_python() -> Option<String> {
    python_candidates().iter().find_map(|(exe, pre)| {
        let out = command(Path::new(exe))
            .args(*pre)
            .arg("--version")
            .stdin(Stdio::null())
            .output()
            .ok()?;
        // Python 2 printed its version on stderr; 3.x uses stdout.
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && text.starts_with("Python 3")).then_some(text)
    })
}

/// Create the venv with the first system Python that works.
fn create_venv(venv: &Path) -> Result<()> {
    let mut last_err = anyhow!("no se encontró Python");
    for (exe, pre) in python_candidates() {
        let mut cmd = command(Path::new(exe));
        cmd.args(*pre).arg("-m").arg("venv").arg(venv);
        match run(&mut cmd, "creando venv") {
            Ok(()) => return Ok(()),
            Err(e) => last_err = e,
        }
    }
    Err(anyhow!("los modelos locales necesitan Python 3: {PYTHON_HINT} ({last_err})"))
}

fn run(cmd: &mut Command, what: &str) -> Result<()> {
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("{what}: no se pudo ejecutar"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
    Err(anyhow!("{what} falló: {tail}"))
}

/// A `Command` with UTF-8 stdio and, on Windows, no flashing console window.
fn command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8");
    // A changed child PATH is also the one used to look up `program`.
    #[cfg(target_os = "macos")]
    cmd.env("PATH", macos_path(std::env::var_os("PATH")));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `PATH` with the usual Python install dirs first, then the inherited one.
#[cfg(target_os = "macos")]
fn macos_path(inherited: Option<std::ffi::OsString>) -> std::ffi::OsString {
    let extra = MACOS_PYTHON_DIRS.iter().map(PathBuf::from);
    let rest = inherited
        .as_deref()
        .map(|p| std::env::split_paths(p).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let dirs: Vec<PathBuf> = extra.chain(rest).filter(|d| seen.insert(d.clone())).collect();
    std::env::join_paths(dirs).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_prefixed_ids_are_local() {
        assert_eq!(model_name("local:medium"), Some("medium"));
        assert_eq!(model_name("whisper-1"), None);
        assert_eq!(model_name("gpt-4o-mini-transcribe"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_path_puts_python_dirs_first_without_duplicates() {
        let path = macos_path(Some("/usr/bin:/opt/homebrew/bin:/bin".into()));
        assert_eq!(
            path,
            "/opt/homebrew/bin:/usr/local/bin:/Library/Frameworks/Python.framework/Versions/Current/bin:/usr/bin:/bin"
        );
    }
}
