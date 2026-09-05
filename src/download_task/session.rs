use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::ThreadId;

use super::DownloadTaskError;

static SESSIONS: OnceLock<Mutex<HashMap<i64, Arc<Session>>>> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopReason {
    Pause,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Downloading,
    Merging,
    Finishing,
}

struct State {
    phase: Phase,
    stop: Option<StopReason>,
    worker: Option<ThreadId>,
    finished: Option<Result<(), DownloadTaskError>>,
}

/// 每个任务只能登记一个会话；登记覆盖进程、读取线程及最后一次回调的完整生命周期。
pub(crate) struct Session {
    task_id: i64,
    pub(crate) stopped: Arc<AtomicBool>,
    state: Mutex<State>,
    completion: Condvar,
}

impl Session {
    pub(crate) fn reserve(task_id: i64) -> Result<Arc<Self>, DownloadTaskError> {
        let mut sessions = SESSIONS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| DownloadTaskError::Poisoned)?;
        if sessions.contains_key(&task_id) {
            return Err(DownloadTaskError::InvalidDownloadRequest(
                "任务仍有运行中的会话".to_owned(),
            ));
        }
        let session = Arc::new(Self {
            task_id,
            stopped: Arc::new(AtomicBool::new(false)),
            state: Mutex::new(State {
                phase: Phase::Downloading,
                stop: None,
                worker: None,
                finished: None,
            }),
            completion: Condvar::new(),
        });
        sessions.insert(task_id, Arc::clone(&session));
        Ok(session)
    }

    pub(crate) fn find(task_id: i64) -> Result<Option<Arc<Self>>, DownloadTaskError> {
        Ok(SESSIONS
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| DownloadTaskError::Poisoned)?
            .get(&task_id)
            .cloned())
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.stop == Some(StopReason::Cancel))
    }

    pub(crate) fn set_worker(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.worker = Some(std::thread::current().id());
        }
    }

    pub(crate) fn request_stop(&self, reason: StopReason) -> Result<(), DownloadTaskError> {
        let mut state = self.state.lock().map_err(|_| DownloadTaskError::Poisoned)?;
        if state.phase != Phase::Downloading || state.finished.is_some() {
            return Err(DownloadTaskError::InvalidDownloadRequest(
                "任务已进入合并或结束阶段".to_owned(),
            ));
        }
        if state.stop.is_some_and(|previous| previous != reason) {
            return Err(DownloadTaskError::InvalidDownloadRequest(
                "任务正在执行其他停止操作".to_owned(),
            ));
        }
        state.stop = Some(reason);
        self.stopped.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn pause_and_wait(&self) -> Result<(), DownloadTaskError> {
        if self.state.lock().map_err(|_| DownloadTaskError::Poisoned)?.worker == Some(std::thread::current().id()) {
            return Err(DownloadTaskError::InvalidDownloadRequest(
                "不能在下载回调内同步等待暂停，请使用 request_pause".to_owned(),
            ));
        }
        self.request_stop(StopReason::Pause)?;
        let mut state = self.state.lock().map_err(|_| DownloadTaskError::Poisoned)?;
        while state.finished.is_none() {
            state = self.completion.wait(state).map_err(|_| DownloadTaskError::Poisoned)?;
        }
        state.finished.clone().expect("会话已经结束")
    }

    pub(crate) fn begin_merging(&self) -> Result<(), DownloadTaskError> {
        let mut state = self.state.lock().map_err(|_| DownloadTaskError::Poisoned)?;
        if state.stop.is_some() {
            return Err(DownloadTaskError::Cancelled);
        }
        state.phase = Phase::Merging;
        Ok(())
    }

    /// 结束竞争在同一个锁内裁决；此后不再接受暂停或取消。
    pub(crate) fn begin_finishing(&self) -> Result<Option<StopReason>, DownloadTaskError> {
        let mut state = self.state.lock().map_err(|_| DownloadTaskError::Poisoned)?;
        state.phase = Phase::Finishing;
        Ok(state.stop)
    }

    pub(crate) fn finish(&self, result: Result<(), DownloadTaskError>) {
        // 先移除会话，再唤醒暂停调用方；所有外部回调必须在此之前完成。
        if let Ok(mut sessions) = SESSIONS.get_or_init(Mutex::default).lock() {
            sessions.remove(&self.task_id);
        }
        if let Ok(mut state) = self.state.lock() {
            state.finished = Some(result);
            self.completion.notify_all();
        }
    }
}
