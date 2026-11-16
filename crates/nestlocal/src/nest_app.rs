//! Whether nlclient, the Nest's own app, keeps running: a restart is a new
//! pid. Starved (the gen 2's memory) or hung, nlheartbeatd restarts it, and
//! the unit itself after a few: one restart can be an update or a push, two
//! in an hour is something wrong. Published retained on <base>/nest_app for
//! Home Assistant (bridge.rs), on both generations.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::logf;

const EVERY: Duration = Duration::from_secs(10);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const HOUR: Duration = Duration::from_secs(60 * 60);
/// Restarts in the last hour from which it is a problem.
const TROUBLE: usize = 2;

/// What is published.
pub trait Report: Send + Sync + 'static {
    fn nest_app(&self, running: bool, restarts_day: usize, trouble: bool);
}

/// nlclient's restarts, by when they were seen.
#[derive(Default)]
struct Watch {
    pid: Option<i32>,
    restarts: VecDeque<Instant>,
}

impl Watch {
    /// Takes the pid seen `now` (None: not running): whether anything
    /// published changed.
    fn take(&mut self, pid: Option<i32>, now: Instant) -> bool {
        let mut changed = false;
        if let Some(pid) = pid {
            if self.pid.is_some_and(|was| was != pid) {
                self.restarts.push_back(now);
                changed = true;
            }
            changed |= self.pid.is_none();
            self.pid = Some(pid);
        } else if self.pid.take().is_some() {
            changed = true;
        }
        while self.restarts.front().is_some_and(|&at| now.duration_since(at) > DAY) {
            self.restarts.pop_front();
            changed = true;
        }
        changed
    }

    fn trouble(&self, now: Instant) -> bool {
        self.restarts.iter().filter(|&&at| now.duration_since(at) <= HOUR).count() >= TROUBLE
    }
}

pub fn start(report: Arc<dyn Report>) {
    let spawned = std::thread::Builder::new().name("nest_app".into()).spawn(move || {
        let mut watch = Watch::default();
        let mut trouble = false;
        loop {
            let now = Instant::now();
            let pid = crate::pids("nlclient").first().copied();
            let was = watch.restarts.len();
            let changed = watch.take(pid, now);
            if watch.restarts.len() > was {
                logf!("nest app: nlclient restarted ({} in the last day)", watch.restarts.len());
            }
            let now_trouble = watch.trouble(now);
            if changed || now_trouble != trouble {
                trouble = now_trouble;
                report.nest_app(watch.pid.is_some(), watch.restarts.len(), trouble);
            }
            std::thread::sleep(EVERY);
        }
    });
    if let Err(e) = spawned {
        logf!("nest app: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_pid_is_a_restart_and_two_in_an_hour_is_trouble() {
        let mut w = Watch::default();
        let t = Instant::now();
        assert!(w.take(Some(10), t), "first seen: published");
        assert!(!w.take(Some(10), t + EVERY));
        assert!(w.take(Some(11), t + Duration::from_secs(60)));
        assert_eq!(w.restarts.len(), 1);
        assert!(!w.trouble(t + Duration::from_secs(60)), "one restart: an update, a push");
        assert!(w.take(None, t + Duration::from_secs(70)), "gone");
        assert!(w.take(Some(12), t + Duration::from_secs(90)), "back, after not running");
        assert_eq!(w.restarts.len(), 1, "not running in between is not counted twice");
        assert!(w.take(Some(13), t + Duration::from_secs(120)));
        assert!(w.trouble(t + Duration::from_secs(120)));
        assert!(!w.trouble(t + HOUR + Duration::from_secs(200)), "an hour on, settled");
        assert!(w.take(Some(13), t + DAY + Duration::from_secs(200)), "a day on, forgotten");
        assert!(w.restarts.is_empty());
    }
}
