use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use super::error::DownloadTaskError;
use super::model::{ClientConfig, YtDlpVersion};

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
