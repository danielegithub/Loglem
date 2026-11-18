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
            let mut backup_task: Option<JoinHandle<()>> = None;
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
                // Controllo di salute a intervalli (default 5 minuti), non a ogni lettura:
                // salva lo stato per gli altri programmi e avvisa se qualcosa è cambiato.
                if crate::monitor::due(&dir) {
                    if let Err(error) = crate::monitor::run(&dir) {
                        crate::diagnostics::error("monitor", "check", None, &error);
                    }
                }
                // Una mail rimasta in coda (errore o limite orario) si riprova qui.
                crate::email::retry(&dir);
                if backup_task.as_ref().is_none_or(|task| task.is_finished())
                    && crate::config::load(&dir).is_ok_and(|c| {
                        c.rules
                            .remote
                            .is_some_and(|plan| plan.due(&crate::remote_backup::status(&dir)))
                    })
                {
                    let folder = dir.clone();
                    backup_task = Some(thread::spawn(move || {
                        if let Err(error) = crate::remote_backup::run(&folder, false) {
                            crate::diagnostics::error("backup", "transfer", None, &error);
                        }
                    }));
                }
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
    /// Chiede un ultimo giro completo di lettura e lo aspetta, al massimo per `limit`.
    /// Se un giro è già in corso, aspetta anche quello successivo: deve partire dopo la richiesta.
    pub fn finish(&self, limit: Duration) -> bool {
        let start = self.progress();
        let target = start.generation + if start.running { 2 } else { 1 };
        self.request();
        let deadline = std::time::Instant::now() + limit;
        while std::time::Instant::now() < deadline {
            if self.progress().generation >= target {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
        false
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
