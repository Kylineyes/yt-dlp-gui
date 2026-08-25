use std::io::{BufRead, BufReader, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use super::error::DownloadTaskError;
use super::model::{ClientConfig, DownloadRequest, YtDlpVersion, DEFAULT_PROGRESS_DELTA};
use super::parser::{self, DownloadOutput};

/// 版本按原始字符串保存，避免把 yt-dlp 的日期版本误当作 SemVer。
pub(crate) fn verify_executable(config: &ClientConfig) -> Result<YtDlpVersion, DownloadTaskError> {
    ensure_executable(&config.yt_dlp_path)?;
    let output = Command::new(&config.yt_dlp_path)
        .arg("--version")
        .output()
        .map_err(DownloadTaskError::Spawn)?;
    if !output.status.success() {
        return Err(DownloadTaskError::VersionCommandFailed {
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() {
        return Err(DownloadTaskError::VersionOutputEmpty);
    }
    Ok(YtDlpVersion { value })
}

pub(crate) fn run_metadata(
    config: ClientConfig,
    url: String,
    cancelled: Arc<AtomicBool>,
) -> Result<Vec<u8>, DownloadTaskError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(DownloadTaskError::Cancelled);
    }
    ensure_executable(&config.yt_dlp_path)?;
    if cancelled.load(Ordering::Acquire) {
        return Err(DownloadTaskError::Cancelled);
    }
    let mut command = Command::new(&config.yt_dlp_path);
    command.args([
        "--encoding",
        "utf-8",
        "--dump-single-json",
        "--skip-download",
        "--no-warnings",
        "--no-playlist",
    ]);
    if let Some(proxy) = config.proxy.as_deref() {
        command.args(["--proxy", proxy]);
    }
    command.arg("--").arg(url);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command.spawn().map_err(DownloadTaskError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DownloadTaskError::Io(std::io::Error::other("缺少 stdout 管道")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DownloadTaskError::Io(std::io::Error::other("缺少 stderr 管道")))?;
    // stdout 和 stderr 必须并行读取，否则大输出可能填满管道并阻塞子进程。
    let stdout_thread = thread::spawn(move || read_pipe(stdout));
    let stderr_thread = thread::spawn(move || read_pipe(stderr));
    let started_at = Instant::now();

    #[derive(Clone, Copy)]
    enum Termination {
        Exited(std::process::ExitStatus),
        Cancelled,
        TimedOut,
    }

    // 轮询让取消和超时无需等待 yt-dlp 自己结束；实际终止后仍 wait 回收句柄。
    let termination = loop {
        if cancelled.load(Ordering::Acquire) {
            kill_child(&mut child);
            break Termination::Cancelled;
        }
        if config.timeout.is_some_and(|timeout| started_at.elapsed() >= timeout) {
            kill_child(&mut child);
            break Termination::TimedOut;
        }
        match child.try_wait().map_err(DownloadTaskError::Io)? {
            Some(status) => break Termination::Exited(status),
            None => thread::sleep(std::time::Duration::from_millis(25)),
        }
    };
    let status = match termination {
        Termination::Exited(status) => status,
        Termination::Cancelled | Termination::TimedOut => child.wait().map_err(DownloadTaskError::Io)?,
    };
    let stdout = join_output(stdout_thread)?;
    let stderr = join_output(stderr_thread)?;

    match termination {
        Termination::Cancelled => Err(DownloadTaskError::Cancelled),
        Termination::TimedOut => Err(DownloadTaskError::Timeout(
            config.timeout.expect("超时终止只会在配置了超时时间时发生"),
        )),
        Termination::Exited(_) if !status.success() => Err(DownloadTaskError::ProcessFailed {
            status: status.code(),
            stderr,
        }),
        Termination::Exited(_) => Ok(stdout.into_bytes()),
    }
}

pub(crate) fn run_download<F>(
    config: ClientConfig,
    request: DownloadRequest,
    cancelled: Arc<AtomicBool>,
    mut on_output: F,
) -> Result<Option<String>, DownloadTaskError>
where
    F: FnMut(DownloadOutput) -> Result<(), DownloadTaskError>,
{
    request.validate().map_err(DownloadTaskError::InvalidDownloadRequest)?;
    if cancelled.load(Ordering::Acquire) {
        return Err(DownloadTaskError::Cancelled);
    }
    ensure_executable(&config.yt_dlp_path)?;

    let mut command = Command::new(&config.yt_dlp_path);
    let progress_delta = DEFAULT_PROGRESS_DELTA.to_string();
    command.args([
        "--encoding",
        "utf-8",
        "--check-formats",
        "--newline",
        "--progress",
        "--progress-delta",
        &progress_delta,
        "--progress-template",
        &parser::progress_template(),
        "--progress-template",
        parser::postprocess_template(),
        "--print",
        "after_move:after_move:%(filepath)s",
        "--no-simulate",
        "--paths",
        &format!("home:{}", request.target_directory.display()),
        "--paths",
        &format!("temp:{}", request.temporary_directory.display()),
        "--output",
        &request.output_template,
        "--continue",
        "--no-overwrites",
        "--part",
        "--merge-output-format",
        &request.merge_output_format,
    ]);
    if let Some(proxy) = config.proxy.as_deref() {
        command.args(["--proxy", proxy]);
    }
    if let Some(ffmpeg_path) = config.ffmpeg_path.as_deref() {
        command.arg("--ffmpeg-location").arg(ffmpeg_path);
    }
    if let Some(value) = &request.options.rate_limit {
        command.args(["--limit-rate", value]);
    }
    for (name, value) in [
        ("--retries", request.options.retries),
        ("--fragment-retries", request.options.fragment_retries),
        ("--file-access-retries", request.options.file_access_retries),
        ("--concurrent-fragments", request.options.concurrent_fragments),
    ] {
        if let Some(value) = value {
            command.args([name, &value.to_string()]);
        }
    }
    command
        .arg("-f")
        .arg(format!(
            "{}+{}",
            request.selected_video_format_id, request.selected_audio_format_id
        ))
        .arg("--")
        .arg(&request.source_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(DownloadTaskError::Spawn)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DownloadTaskError::Io(std::io::Error::other("缺少 stdout 管道")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DownloadTaskError::Io(std::io::Error::other("缺少 stderr 管道")))?;
    let (line_sender, line_receiver) = mpsc::channel();
    let stdout_thread = spawn_line_reader(stdout, line_sender.clone(), PipeKind::Stdout);
    let stderr_thread = spawn_line_reader(stderr, line_sender, PipeKind::Stderr);

    let started_at = Instant::now();
    let mut output_path = None;
    let mut stderr_summary = String::new();
    enum Termination {
        Exited(std::process::ExitStatus),
        Cancelled,
        TimedOut,
        Failed(DownloadTaskError),
    }
    let termination = loop {
        if cancelled.load(Ordering::Acquire) {
            kill_child(&mut child);
            break Termination::Cancelled;
        }
        if config.timeout.is_some_and(|timeout| started_at.elapsed() >= timeout) {
            kill_child(&mut child);
            break Termination::TimedOut;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Termination::Exited(status),
            Err(error) => {
                kill_child(&mut child);
                break Termination::Failed(DownloadTaskError::Io(error));
            }
            Ok(None) => match line_receiver.recv_timeout(std::time::Duration::from_millis(25)) {
                Ok(line) => {
                    if let Err(error) =
                        handle_pipe_line(line, &request, &mut output_path, &mut stderr_summary, &mut on_output)
                    {
                        kill_child(&mut child);
                        break Termination::Failed(error);
                    }
                }
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {}
            },
        }
    };
    let status = match &termination {
        Termination::Exited(status) => Some(*status),
        Termination::Cancelled | Termination::TimedOut | Termination::Failed(_) => {
            child.wait().map_err(DownloadTaskError::Io)?;
            None
        }
    };
    stdout_thread.join().map_err(|_| DownloadTaskError::WorkerPanicked)?;
    stderr_thread.join().map_err(|_| DownloadTaskError::WorkerPanicked)?;
    let mut drain_error = None;
    while let Ok(line) = line_receiver.try_recv() {
        if drain_error.is_some() {
            if line.kind == PipeKind::Stderr {
                if let Ok(value) = line.value {
                    append_stderr(&mut stderr_summary, &value);
                }
            }
            continue;
        }
        if let Err(error) = handle_pipe_line(line, &request, &mut output_path, &mut stderr_summary, &mut on_output) {
            drain_error = Some(error);
        }
    }
    match termination {
        Termination::Cancelled => return Err(DownloadTaskError::Cancelled),
        Termination::TimedOut => {
            return Err(DownloadTaskError::Timeout(
                config.timeout.expect("超时终止只会在配置了超时时间时发生"),
            ))
        }
        Termination::Failed(error) => return Err(error),
        Termination::Exited(_) => {
            if let Some(error) = drain_error {
                return Err(error);
            }
        }
    }
    if !status.is_some_and(|status| status.success()) {
        return Err(DownloadTaskError::DownloadProcessFailed {
            status: status.and_then(|status| status.code()),
            stderr: stderr_summary,
        });
    }
    Ok(output_path)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PipeKind {
    Stdout,
    Stderr,
}

struct PipeLine {
    kind: PipeKind,
    value: Result<String, std::io::Error>,
}

fn spawn_line_reader<R>(reader: R, sender: mpsc::Sender<PipeLine>, kind: PipeKind) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) => break,
                Ok(_) => {
                    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                        bytes.pop();
                    }
                    let value = String::from_utf8_lossy(&bytes).into_owned();
                    if sender.send(PipeLine { kind, value: Ok(value) }).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(PipeLine {
                        kind,
                        value: Err(error),
                    });
                    break;
                }
            }
        }
    })
}

fn handle_pipe_line<F>(
    pipe_line: PipeLine,
    request: &DownloadRequest,
    output_path: &mut Option<String>,
    stderr_summary: &mut String,
    on_output: &mut F,
) -> Result<(), DownloadTaskError>
where
    F: FnMut(DownloadOutput) -> Result<(), DownloadTaskError>,
{
    let line = pipe_line.value.map_err(DownloadTaskError::Io)?;
    if pipe_line.kind == PipeKind::Stderr {
        append_stderr(stderr_summary, &line);
    }
    if line.starts_with("download:") || line.starts_with("postprocess:") || line.starts_with("after_move:") {
        handle_download_line(&line, request, output_path, on_output)?;
    }
    Ok(())
}

fn append_stderr(stderr: &mut String, line: &str) {
    const MAX_STDERR_CHARS: usize = 4096;
    let mut current = stderr.chars().count();
    if current >= MAX_STDERR_CHARS {
        return;
    }
    if !stderr.is_empty() {
        stderr.push('\n');
        current += 1;
    }
    stderr.extend(line.chars().take(MAX_STDERR_CHARS.saturating_sub(current)));
}

fn handle_download_line<F>(
    line: &str,
    request: &DownloadRequest,
    output_path: &mut Option<String>,
    on_output: &mut F,
) -> Result<(), DownloadTaskError>
where
    F: FnMut(DownloadOutput) -> Result<(), DownloadTaskError>,
{
    if let Some(output) = parser::parse_download_line(
        line,
        &request.selected_video_format_id,
        &request.selected_audio_format_id,
    )? {
        if let DownloadOutput::OutputPath(path) = &output {
            *output_path = Some(path.clone());
        }
        catch_unwind(AssertUnwindSafe(|| on_output(output))).map_err(|_| DownloadTaskError::WorkerPanicked)??;
    }
    Ok(())
}

fn ensure_executable(path: &std::path::Path) -> Result<(), DownloadTaskError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(DownloadTaskError::ExecutableNotFound(path.to_owned()))
    }
}

fn kill_child(child: &mut Child) {
    let _ = child.kill();
}

fn read_pipe<R: Read>(mut reader: R) -> Result<String, std::io::Error> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

fn join_output(handle: thread::JoinHandle<Result<String, std::io::Error>>) -> Result<String, DownloadTaskError> {
    handle
        .join()
        .map_err(|_| DownloadTaskError::WorkerPanicked)?
        .map_err(DownloadTaskError::Io)
}
