//! One collector for the web process; independent of open browser tabs.
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub running: bool,
    pub requested: bool,
    pub last_finished: Option<String>,
    pub error: Option<String>,
    pub generation: u64,
    #[serde(skip)]
    stop: bool,
}
pub struct Worker {
    state: Arc<(Mutex<Progress>, Condvar)>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn start(dir: PathBuf) -> Self {
        let state = Arc::new((
            Mutex::new(Progress {
                requested: true,
                ..Progress::default()
            }),
            Condvar::new(),
        ));
        let thread_state = state.clone();
        let join = thread::spawn(move || {
            let (mutex, wake) = &*thread_state;
            loop {
                {
                    let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
                    if state.stop {
                        break;
                    }
                    state.running = true;
                    state.requested = false;
                }
                let result = std::panic::catch_unwind(|| crate::collector::once(&dir))
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "Arresto anomalo della raccolta. La traccia è in «Problemi»."
                        ))
                    });
                let seconds = crate::config::load(&dir)
                    .map(|c| c.poll_seconds)
                    .unwrap_or(5);
                let mut state = mutex.lock().unwrap_or_else(|p| p.into_inner());
                state.running = false;
                state.generation += 1;
                state.error = result.err().map(|e| format!("{e:#}"));
                state.last_finished =
                    Some(crate::timestamps::format_ns(crate::timestamps::now_ns()));
                if state.stop {
                    break;
                }
                if !state.requested {
                    let (guard, _) = wake
                        .wait_timeout_while(state, Duration::from_secs(seconds), |s| {
                            !s.requested && !s.stop
                        })
                        .unwrap_or_else(|p| p.into_inner());
                    if guard.stop {
                        break;
                    }
                }
            }
        });
        Self {
            state,
            join: Some(join),
        }
    }
    pub fn request(&self) {
        let (mutex, wake) = &*self.state;
        mutex.lock().unwrap_or_else(|p| p.into_inner()).requested = true;
        wake.notify_one();
    }
    pub fn progress(&self) -> Progress {
        self.state
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.state.0.lock().unwrap_or_else(|p| p.into_inner()).stop = true;
        self.state.1.notify_one();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
