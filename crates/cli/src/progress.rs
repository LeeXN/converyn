//! Human progress belongs on stderr; stdout remains the command's original result.
use converyn_runtime::artifacts::transfer::Progress as Transfer;
use std::{
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Preparing,
    Hashing,
    Ready,
    Checking,
    Uploading,
    Verifying,
    Reused,
    Uploaded,
    Registering,
}
struct State {
    stage: Stage,
    generation: u64,
    file: String,
    file_size: u64,
    current: u64,
    current_finished: bool,
    files: usize,
    completed_files: usize,
    total: u64,
    completed: u64,
    uploaded_files: usize,
    reused_files: usize,
    uploaded_bytes: u64,
    started: Instant,
    phase_started: Instant,
    transfer_started: Instant,
    stopped: bool,
    success: Option<String>,
}
impl State {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            stage: Stage::Preparing,
            generation: 0,
            file: String::new(),
            file_size: 0,
            current: 0,
            current_finished: false,
            files: 0,
            completed_files: 0,
            total: 0,
            completed: 0,
            uploaded_files: 0,
            reused_files: 0,
            uploaded_bytes: 0,
            started: now,
            phase_started: now,
            transfer_started: now,
            stopped: false,
            success: None,
        }
    }
    fn stage(&mut self, stage: Stage) {
        if self.stage != stage {
            self.stage = stage;
            self.generation += 1;
            self.phase_started = Instant::now();
        }
    }
    fn transfer(&mut self, event: Transfer) {
        match event {
            Transfer::Checking => self.stage(Stage::Checking),
            Transfer::Uploading { confirmed_bytes } => {
                self.current = confirmed_bytes.min(self.file_size);
                self.stage(Stage::Uploading);
            }
            Transfer::Verifying => {
                self.current = self.file_size;
                self.stage(Stage::Verifying);
            }
            Transfer::Reused | Transfer::Completed => {
                if !self.current_finished {
                    self.current_finished = true;
                    self.completed = self.completed.saturating_add(self.file_size);
                    self.completed_files += 1;
                    if event == Transfer::Reused {
                        self.reused_files += 1;
                    } else {
                        self.uploaded_files += 1;
                        self.uploaded_bytes = self.uploaded_bytes.saturating_add(self.file_size);
                    }
                }
                self.current = 0;
                self.stage(if event == Transfer::Reused {
                    Stage::Reused
                } else {
                    Stage::Uploaded
                });
            }
        }
    }
}

pub(crate) struct Progress {
    shared: Arc<(Mutex<State>, Condvar)>,
    worker: Option<JoinHandle<()>>,
    root: PathBuf,
}
impl Progress {
    pub(crate) fn new(source: &Path) -> Self {
        let root = if source.is_dir() {
            source
        } else {
            source.parent().unwrap_or(Path::new("."))
        };
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let shared = Arc::new((Mutex::new(State::new()), Condvar::new()));
        let output = shared.clone();
        let worker = thread::spawn(move || draw(output));
        Self {
            shared,
            worker: Some(worker),
            root,
        }
    }
    fn update(&self, update: impl FnOnce(&mut State)) {
        let (lock, wake) = &*self.shared;
        if let Ok(mut state) = lock.lock() {
            let generation = state.generation;
            update(&mut state);
            if generation != state.generation || state.stopped {
                wake.notify_one();
            }
        }
    }
    fn label(&self, path: &Path) -> String {
        let path = path.strip_prefix(&self.root).unwrap_or(path);
        crate::output::clean(&path.display().to_string())
    }
    pub(crate) fn hashing(&self, path: &Path, read: u64, total: u64) {
        let label = self.label(path);
        self.update(|state| {
            if state.stage != Stage::Hashing || state.file != label {
                state.file = label;
                state.stage = Stage::Hashing;
                state.generation += 1;
                state.phase_started = Instant::now();
            }
            state.current = read;
            state.file_size = total;
        });
    }
    pub(crate) fn uploads(&self, files: usize, total: u64) {
        self.update(|state| {
            state.files = files;
            state.total = total;
            state.current = 0;
            state.file.clear();
            state.transfer_started = Instant::now();
            state.stage(Stage::Ready);
        });
    }
    pub(crate) fn file(&self, path: &Path, size: u64) {
        let label = self.label(path);
        self.update(|state| {
            state.file = label;
            state.file_size = size;
            state.current = 0;
            state.current_finished = false;
            state.generation += 1;
            state.stage(Stage::Checking);
        });
    }
    pub(crate) fn transfer(&self, event: Transfer) {
        self.update(|state| state.transfer(event));
    }
    pub(crate) fn registering(&self) {
        self.update(|state| state.stage(Stage::Registering));
    }
    pub(crate) fn finish(&mut self, message: impl Into<String>) {
        self.update(|state| {
            state.success = Some(message.into());
            state.stopped = true;
        });
        self.join();
    }
    fn join(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Progress {
    fn drop(&mut self) {
        self.update(|state| state.stopped = true);
        self.join();
    }
}
fn bytes(value: u64) -> String {
    if value >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", value as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if value >= 1024 * 1024 {
        format!("{:.1} MiB", value as f64 / (1024.0 * 1024.0))
    } else if value >= 1024 {
        format!("{:.1} KiB", value as f64 / 1024.0)
    } else {
        format!("{value} B")
    }
}
fn elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    }
}
fn bar(current: u64, total: u64, frame: usize) -> String {
    if total == 0 {
        return format!("[{}]", ["|", "/", "-", "\\"][frame % 4]);
    }
    let percent = (u128::from(current.min(total)) * 100 / u128::from(total)) as usize;
    let filled = percent * 14 / 100;
    format!(
        "[{}{}] {percent:3}%",
        "=".repeat(filled),
        "-".repeat(14 - filled)
    )
}
fn line(state: &State, now: Instant, frame: usize, width: usize) -> String {
    let wait = elapsed(now.duration_since(state.phase_started));
    let line = match state.stage {
        Stage::Preparing => format!("Reading workflow {}  {wait}", bar(0, 0, frame)),
        Stage::Hashing => format!(
            "Hashing {} {}/{}  {}  {wait}",
            bar(state.current, state.file_size, frame),
            bytes(state.current),
            bytes(state.file_size),
            state.file
        ),
        Stage::Ready => format!("Artifacts: {} files, {}", state.files, bytes(state.total)),
        Stage::Registering => format!("Registering workflow {}  {wait}", bar(0, 0, frame)),
        _ => {
            let done = state.completed.saturating_add(if state.current_finished {
                0
            } else {
                state.current
            });
            let phase = match state.stage {
                Stage::Checking => "checking replicas",
                Stage::Uploading => "uploading",
                Stage::Verifying => "verifying replicas",
                Stage::Reused => "reused",
                Stage::Uploaded => "uploaded",
                _ => unreachable!(),
            };
            let rate = if state.stage == Stage::Uploading {
                let uploaded = state.uploaded_bytes.saturating_add(state.current);
                let seconds = now.duration_since(state.transfer_started).as_secs_f64();
                if seconds >= 0.1 && uploaded > 0 {
                    format!(" {}/s", bytes((uploaded as f64 / seconds) as u64))
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            let filename = Path::new(&state.file)
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_else(|| state.file.as_str().into());
            format!(
                "{phase} {} {} {}/{} {}/{} files{rate} ({wait})",
                crate::output::truncate(&filename, 24),
                bar(done, state.total, frame),
                bytes(done),
                bytes(state.total),
                state.completed_files,
                state.files
            )
        }
    };
    crate::output::truncate(&line, width.saturating_sub(1).max(1))
}
fn stderr_width() -> usize {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    // SAFETY: ioctl writes winsize into valid, correctly sized initialized storage.
    if unsafe { libc::ioctl(libc::STDERR_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) } == 0 {
        // SAFETY: successful TIOCGWINSZ initialized the winsize above.
        let size = unsafe { size.assume_init() };
        if size.ws_col > 0 {
            return usize::from(size.ws_col);
        }
    }
    100
}
fn draw(shared: Arc<(Mutex<State>, Condvar)>) {
    let interactive =
        std::io::stderr().is_terminal() && std::env::var("TERM").as_deref() != Ok("dumb");
    let (lock, wake) = &*shared;
    let mut last = Instant::now() - Duration::from_secs(10);
    let mut last_generation = u64::MAX;
    let mut frame = 0;
    let mut drawn = false;
    loop {
        let Ok(state) = lock.lock() else { break };
        let now = Instant::now();
        if state.stopped {
            let message = state.success.as_ref().map(|message| {
                if state.files > 0 {
                    format!(
                        "{message} — {} uploaded, {} reused, {} ({})",
                        state.uploaded_files,
                        state.reused_files,
                        bytes(state.completed),
                        elapsed(now.duration_since(state.started))
                    )
                } else {
                    message.clone()
                }
            });
            drop(state);
            let mut stderr = std::io::stderr().lock();
            if interactive && drawn {
                let _ = write!(stderr, "\r\x1b[2K");
            }
            if let Some(message) = message {
                let _ = writeln!(stderr, "{message}");
            }
            let _ = stderr.flush();
            break;
        }
        let interval = if interactive {
            Duration::from_millis(120)
        } else {
            Duration::from_secs(5)
        };
        let changed = state.generation != last_generation;
        if changed || now.duration_since(last) >= interval {
            let text = line(
                &state,
                now,
                frame,
                if interactive { stderr_width() } else { 200 },
            );
            last_generation = state.generation;
            last = now;
            frame += 1;
            drop(state);
            let mut stderr = std::io::stderr().lock();
            if interactive {
                let _ = write!(stderr, "\r\x1b[2K{text}");
            } else {
                let _ = writeln!(stderr, "{text}");
            }
            let _ = stderr.flush();
            drawn = true;
        } else {
            // Release the lock while waiting; data updates are coalesced, not queued per block.
            let _ = wake.wait_timeout(state, Duration::from_millis(120));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_byte_ack_is_distinct_from_durable_replica_completion() {
        let mut state = State::new();
        state.files = 1;
        state.total = 2048;
        state.file_size = 2048;
        state.file = "payload.bin".into();
        state.transfer(Transfer::Uploading {
            confirmed_bytes: 2048,
        });
        state.transfer(Transfer::Verifying);
        let text = line(&state, Instant::now(), 0, 240);
        assert!(
            text.contains("100%")
                && text.contains("0/1 files")
                && text.contains("verifying replicas")
        );
        assert_eq!(state.completed_files, 0);
        state.transfer(Transfer::Completed);
        state.transfer(Transfer::Completed);
        assert_eq!(
            (state.completed_files, state.uploaded_files, state.completed),
            (1, 1, 2048)
        );
    }
    #[test]
    fn reused_payloads_are_not_counted_as_uploaded_bytes() {
        let mut state = State::new();
        state.files = 1;
        state.total = 1234;
        state.file_size = 1234;
        state.transfer(Transfer::Checking);
        state.transfer(Transfer::Reused);
        assert_eq!(
            (
                state.reused_files,
                state.uploaded_files,
                state.uploaded_bytes
            ),
            (1, 0, 0)
        );
        assert_eq!(state.completed, 1234);
        let text = line(&state, Instant::now(), 0, 240);
        assert!(text.contains("reused") && !text.contains("/s"));
    }
}
