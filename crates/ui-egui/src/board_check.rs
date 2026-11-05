//! Board checks off the frame: a Project settings change that can alter the copper findings
//! (design rules, clearances, net classes, board, stackup, fabrication limits, machines)
//! starts the board checks on a worker and keeps the old findings on screen until they land.
//!
//! Context: #1672. After #1638 the checks reuse their last run when nothing they read has
//! changed. A setting the checks do read must run them again, and on a large board that took
//! seconds on the window thread. Here the window answers at once, the footer says Checking
//! and the new findings replace the old ones on the frame after the worker is done.
//!
//! # Rules
//! - Only the settings paths defer (`with_deferred_checks`); every other command still
//!   checks before it returns.
//! - A result is used only for the design it was made for: `Session::checks_if_ready` takes
//!   it when the design still matches, and a newer design starts a new run.
//! - The browser build has no threads: the run waits one painted frame so the footer shows
//!   Checking, then runs on the window thread.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`Session::compute_board_checks`, `keep_board_checks`,
//!   `checks_if_ready`)
//! - crates/ui-egui/src/lib.rs (`App::refresh`)
//!
//! # Do not
//! - Run the board checks here; call the engine.
//! - Block the frame waiting for the worker outside tests.
use crate::App;
use pcbcraft_doc::Project;
use pcbcraft_engine::{BoardChecks, Session};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

/// A board check run that has not landed yet.
pub(crate) struct Pending {
    /// Where the worker's answer arrives.
    rx: Receiver<BoardChecks>,
    /// The design to check, for a build with no threads; run after one painted frame.
    later: Option<(Project, bool)>,
}

impl App {
    /// Run `work` with the checks of every refresh inside it deferred to a worker when they
    /// have to run from scratch.
    pub(crate) fn with_deferred_checks(&mut self, work: impl FnOnce(&mut Self)) {
        let before = std::mem::replace(&mut self.defer_checks, true);
        work(self);
        self.defer_checks = before;
    }

    /// Whether board checks are running for a design newer than the findings on screen.
    pub(crate) fn checking(&self) -> bool {
        self.board_check.is_some()
    }

    /// Start the board checks of `project` away from the frame.
    pub(super) fn start_board_check(&mut self, project: Project) {
        let (tx, rx) = channel();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let repaint = self.repaint.clone();
            let started = std::thread::Builder::new()
                .name("pcbcraft-checks".into())
                .spawn(move || {
                    let _ = tx.send(Session::compute_board_checks(&project));
                    if let Some(ctx) = repaint {
                        ctx.request_repaint();
                    }
                });
            if started.is_err() {
                self.board_check = None;
                self.checks_now();
                return;
            }
            self.board_check = Some(Pending { rx, later: None });
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.board_check = Some(Pending {
                rx,
                later: Some((project, false)),
            });
            let _ = tx;
        }
    }

    /// Check on the window thread and show the result: the way out when no worker starts.
    fn checks_now(&mut self) {
        if let Some(checks) = self.session.lock().ok().map(|s| s.checks()) {
            self.checks = checks;
        }
    }

    /// Take the worker's answer if it has landed.
    ///
    /// # Rules
    /// - A design that moved on while the worker ran starts a new run and keeps the old
    ///   findings.
    pub(super) fn poll_board_check(&mut self) {
        let Some(pending) = self.board_check.as_mut() else {
            return;
        };
        if let Some((project, painted)) = pending.later.take() {
            if !painted {
                pending.later = Some((project, true));
                return;
            }
            let (tx, rx) = channel();
            let _ = tx.send(Session::compute_board_checks(&project));
            pending.rx = rx;
        }
        let done = match pending.rx.try_recv() {
            Ok(done) => done,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                // The worker stopped without an answer: check here rather than ask again.
                self.board_check = None;
                self.checks_now();
                return;
            }
        };
        self.board_check = None;
        let (checks, rerun) = self
            .session
            .lock()
            .ok()
            .map(|s| {
                s.keep_board_checks(done);
                match s.checks_if_ready() {
                    Some(checks) => (Some(checks), None),
                    None => (None, Some(s.board_check_input().0)),
                }
            })
            .unwrap_or_default();
        if let Some(checks) = checks {
            self.checks = checks;
        }
        if let Some(project) = rerun {
            self.start_board_check(project);
        }
    }

    /// Wait for the run in progress and show its findings. For tests: the frame never waits.
    #[cfg(test)]
    pub(crate) fn finish_board_check(&mut self) {
        while self.board_check.is_some() {
            self.poll_board_check();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
