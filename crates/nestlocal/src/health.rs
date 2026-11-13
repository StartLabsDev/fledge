//! The gen 2's memory, watched: it has 58 MB and no swap. As nestlocal
//! grows, the kernel evicts nlclient's code pages; nlclient stalls reading
//! them back from the compressed JFFS2, misses its heartbeats, and
//! nlheartbeatd restarts the unit. So what is watched is nlclient's major
//! page faults (the stall itself), the memory left, and nestlocal's own.
//!
//! - `level()` is what the home screen cards keep to (ui): Tight, they keep
//!   fewer frames; Critical, none but the one shown, and they stand still.
//! - Grown past OWN_HARD_KB, nestlocal stops itself: monit starts it again
//!   in seconds, where the unit restarting takes a minute. The kernel is
//!   told to pick nestlocal first, too, should memory run out whole.
//! - Restarted BOOT_LIMIT times running before staying up STABLE, the cards
//!   are paused (`cards_paused()`), until Home Assistant's Resume cards.
//! - What it reads is published retained on <base>/memory for Home
//!   Assistant: the thresholds below are first guesses, to be set from it.
//!
//! Only on the gen 2 (start): the gen 3 has 512 MB.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::logf;

/// How pressed the unit's memory is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Calm,
    Tight,
    Critical,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Calm => "calm",
            Level::Tight => "tight",
            Level::Critical => "critical",
        }
    }
}

static LEVEL: AtomicU8 = AtomicU8::new(0);
static PAUSED: AtomicBool = AtomicBool::new(false);

/// The level now: Calm wherever memory isn't watched.
pub fn level() -> Level {
    match LEVEL.load(Ordering::Relaxed) {
        0 => Level::Calm,
        1 => Level::Tight,
        _ => Level::Critical,
    }
}

/// Whether the cards are paused: the unit kept restarting with them.
pub fn cards_paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

/// Memory left (free, buffers and page cache: nlclient's code is in the
/// cache) below which the level rises, in kB.
const TIGHT_FREE_KB: u64 = 6_000;
const CRITICAL_FREE_KB: u64 = 3_000;
/// nlclient's major page faults a second from which the level rises: each
/// is a page read back from flash while nlclient waits.
const TIGHT_FAULTS: u64 = 20;
const CRITICAL_FAULTS: u64 = 80;
/// nestlocal's own memory at which the level rises, and at which it stops
/// itself. Using the cards it measured 12 to 14 MB; at 22 the unit restarted.
const OWN_TIGHT_KB: u64 = 17_000;
const OWN_CRITICAL_KB: u64 = 20_000;
const OWN_HARD_KB: u64 = 26_000;
/// Readings a level is held for before it falls.
const HOLD: u32 = 3;
const EVERY: Duration = Duration::from_secs(5);

/// One reading.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reading {
    /// nestlocal's resident memory, kB.
    pub own_kb: u64,
    /// Free memory, buffers and page cache, kB.
    pub free_kb: u64,
    /// nlclient's major page faults since it started; None without it.
    pub faults: Option<u64>,
}

/// /proc/self/statm's resident pages (its second number), in kB.
fn own_kb(statm: &str) -> Option<u64> {
    Some(statm.split_ascii_whitespace().nth(1)?.parse::<u64>().ok()? * 4)
}

/// /proc/meminfo's MemFree, Buffers and Cached, in kB.
fn free_kb(meminfo: &str) -> Option<u64> {
    let field = |name: &str| {
        meminfo.lines().find_map(|line| line.strip_prefix(name)?.strip_prefix(':')?.split_ascii_whitespace().next()?.parse::<u64>().ok())
    };
    Some(field("MemFree")? + field("Buffers").unwrap_or(0) + field("Cached").unwrap_or(0))
}

/// /proc/<pid>/stat's majflt: the twelfth field, the tenth after the
/// command's closing bracket (a command may hold spaces and brackets).
fn major_faults(stat: &str) -> Option<u64> {
    stat[stat.rfind(')')? + 1..].split_ascii_whitespace().nth(9)?.parse().ok()
}

fn read() -> Option<Reading> {
    let own_kb = own_kb(&std::fs::read_to_string("/proc/self/statm").ok()?)?;
    let free_kb = free_kb(&std::fs::read_to_string("/proc/meminfo").ok()?)?;
    let faults = crate::pids("nlclient")
        .first()
        .and_then(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
        .and_then(|stat| major_faults(&stat));
    Some(Reading { own_kb, free_kb, faults })
}

/// The level a reading asks for: `faults` nlclient's major faults a second.
fn judge(reading: &Reading, faults: u64) -> Level {
    if reading.free_kb < CRITICAL_FREE_KB || faults >= CRITICAL_FAULTS || reading.own_kb >= OWN_CRITICAL_KB {
        Level::Critical
    } else if reading.free_kb < TIGHT_FREE_KB || faults >= TIGHT_FAULTS || reading.own_kb >= OWN_TIGHT_KB {
        Level::Tight
    } else {
        Level::Calm
    }
}

/// The level kept from one reading to the next: it rises at once, and
/// falls a step only after HOLD readings asking for less.
#[derive(Default)]
struct Watch {
    level: Option<Level>,
    lower: u32,
    last: Option<Reading>,
}

impl Watch {
    /// Takes a reading `seconds` after the last; the level, and nlclient's
    /// major faults a second since.
    fn take(&mut self, reading: Reading, seconds: u64) -> (Level, u64) {
        // A restarted nlclient counts from 0 again: no rate from that.
        let faults = match (self.last.and_then(|last| last.faults), reading.faults) {
            (Some(before), Some(now)) if now >= before => (now - before) / seconds.max(1),
            _ => 0,
        };
        self.last = Some(reading);
        let asked = judge(&reading, faults);
        let level = self.level.unwrap_or(Level::Calm);
        let level = if asked >= level {
            self.lower = 0;
            asked
        } else {
            self.lower += 1;
            if self.lower < HOLD {
                level
            } else {
                self.lower = 0;
                if level == Level::Critical { Level::Tight } else { Level::Calm }
            }
        };
        self.level = Some(level);
        (level, faults)
    }
}

/// Where the count of restarts is kept, and the cards' pause.
const BOOTS: &str = "/media/data/fledge-boots";
/// Started within this of the unit's start, nestlocal started with it (and
/// not after an update, or stopped by monit).
const BOOT: Duration = Duration::from_secs(180);
/// Up this long, the unit is taken to stay up.
const STABLE: Duration = Duration::from_secs(600);
/// Restarts running, none of them staying up STABLE, that pause the cards.
const BOOT_LIMIT: u32 = 3;

/// What BOOTS holds: the restarts running, and whether the cards are paused.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Boots {
    running: u32,
    paused: bool,
}

impl Boots {
    fn parse(text: &str) -> Boots {
        let mut words = text.split_ascii_whitespace();
        let running = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        Boots { running, paused: words.next() == Some("paused") }
    }

    fn text(self) -> String {
        format!("{}{}\n", self.running, if self.paused { " paused" } else { "" })
    }

    /// As the unit starts once more: one more restart, the cards paused
    /// from BOOT_LIMIT of them.
    fn started(self) -> Boots {
        let running = self.running.saturating_add(1);
        Boots { running, paused: self.paused || running >= BOOT_LIMIT }
    }

    /// As the unit has stayed up: the restarts forgotten; a pause is kept.
    fn stayed_up(self) -> Boots {
        Boots { running: 0, ..self }
    }
}

fn boots() -> Boots {
    Boots::parse(&std::fs::read_to_string(BOOTS).unwrap_or_default())
}

fn keep(boots: Boots) {
    if let Err(e) = std::fs::write(BOOTS, boots.text()) {
        logf!("health: {BOOTS}: {e}");
    }
}

fn uptime() -> Option<Duration> {
    let text = std::fs::read_to_string("/proc/uptime").ok()?;
    Some(Duration::from_secs_f64(text.split_ascii_whitespace().next()?.parse().ok()?))
}

/// Home Assistant's Resume cards: they show again, from now.
pub fn resume_cards() {
    keep(Boots::default());
    if PAUSED.swap(false, Ordering::Relaxed) {
        logf!("health: the cards resume");
    }
}

/// What a reading is published as.
pub trait Report: Send + Sync + 'static {
    fn memory(&self, reading: &Reading, faults: u64, level: Level, paused: bool);
}

/// Starts the watch, on a gen 2. Before the cards start: they ask whether
/// they are paused.
pub fn start(report: Arc<dyn Report>) {
    if crate::device::current().generation != 2 {
        return;
    }
    // Should memory run out whole, nestlocal goes first: monit starts it
    // again, where nlclient killed restarts the unit.
    if let Err(e) = std::fs::write("/proc/self/oom_score_adj", "1000") {
        logf!("health: oom_score_adj: {e}");
    }
    let since_boot = uptime();
    let at_boot = since_boot.is_some_and(|up| up < BOOT);
    let mut kept = boots();
    if at_boot {
        kept = kept.started();
        keep(kept);
    }
    if kept.paused {
        PAUSED.store(true, Ordering::Relaxed);
        logf!("health: the unit restarted {} times running: the home screen cards are paused, until Home Assistant resumes them", kept.running);
    }
    let spawned = std::thread::Builder::new().name("health".into()).spawn(move || {
        let mut watch = Watch::default();
        let mut settled = !at_boot;
        let mut published: Option<(Reading, Level, bool)> = None;
        let mut quiet = 0u32;
        loop {
            std::thread::sleep(EVERY);
            if !settled && uptime().is_some_and(|up| up >= STABLE) {
                settled = true;
                keep(boots().stayed_up());
            }
            let Some(reading) = read() else { continue };
            let was = level();
            let (now, faults) = watch.take(reading, EVERY.as_secs());
            LEVEL.store(now as u8, Ordering::Relaxed);
            if now != was {
                logf!("health: memory is {}: nestlocal {} kB, {} kB left, nlclient {faults} major faults a second",
                      now.name(), reading.own_kb, reading.free_kb);
            }
            if reading.own_kb >= OWN_HARD_KB {
                logf!("health: nestlocal has grown to {} kB: stopping, for monit to start it again", reading.own_kb);
                std::process::exit(1);
            }
            // As it changes by a quarter of a megabyte, or the level or the
            // pause does; once a minute besides.
            let paused = cards_paused();
            quiet += 1;
            let moved = published.is_none_or(|(last, level, was_paused)| {
                level != now || was_paused != paused
                    || last.own_kb.abs_diff(reading.own_kb) >= 256 || last.free_kb.abs_diff(reading.free_kb) >= 256
            });
            if moved || quiet >= 12 {
                quiet = 0;
                published = Some((reading, now, paused));
                report.memory(&reading, faults, now, paused);
            }
        }
    });
    if let Err(e) = spawned {
        logf!("health: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_files_are_read() {
        assert_eq!(own_kb("2101 1533 420 600 0 500 0\n"), Some(6132));
        let meminfo = "MemTotal:          59148 kB\nMemFree:            2204 kB\nBuffers:               0 kB\nCached:            11340 kB\nSwapCached:            0 kB\n";
        assert_eq!(free_kb(meminfo), Some(13544));
        // A command with spaces and a bracket in it.
        let stat = "495 (nl client) x) S 1 495 495 0 -1 4202752 9000 0 1234 0 500 300 0 0 20 0 9 0 2000 60000000 3000";
        assert_eq!(major_faults(stat), Some(1234));
        assert_eq!(major_faults("gone"), None);
    }

    fn reading(own_kb: u64, free_kb: u64, faults: u64) -> Reading {
        Reading { own_kb, free_kb, faults: Some(faults) }
    }

    #[test]
    fn the_level_rises_at_once_and_falls_a_step_after_three_readings() {
        let mut watch = Watch::default();
        assert_eq!(watch.take(reading(8_000, 14_000, 100), 5), (Level::Calm, 0), "no rate from one reading");
        assert_eq!(watch.take(reading(8_000, 14_000, 150), 5), (Level::Calm, 10));
        assert_eq!(watch.take(reading(8_000, 14_000, 650), 5), (Level::Critical, 100), "nlclient stalls on its pages");
        assert_eq!(watch.take(reading(8_000, 14_000, 650), 5).0, Level::Critical);
        assert_eq!(watch.take(reading(8_000, 14_000, 650), 5).0, Level::Critical);
        assert_eq!(watch.take(reading(8_000, 14_000, 650), 5).0, Level::Tight, "a step down after three");
        assert_eq!(watch.take(reading(8_000, 14_000, 800), 5).0, Level::Tight, "30 a second holds it");
        for _ in 0..2 {
            assert_eq!(watch.take(reading(8_000, 14_000, 800), 5).0, Level::Tight);
        }
        assert_eq!(watch.take(reading(8_000, 14_000, 800), 5).0, Level::Calm);
    }

    #[test]
    fn little_memory_left_or_a_grown_nestlocal_raise_the_level() {
        assert_eq!(judge(&reading(8_000, 5_000, 0), 0), Level::Tight);
        assert_eq!(judge(&reading(8_000, 2_500, 0), 0), Level::Critical);
        assert_eq!(judge(&reading(18_000, 14_000, 0), 0), Level::Tight);
        assert_eq!(judge(&reading(21_000, 14_000, 0), 0), Level::Critical);
        assert_eq!(judge(&reading(13_000, 14_000, 0), 5), Level::Calm, "as measured using the cards");
    }

    #[test]
    fn a_restarted_nlclient_gives_no_rate() {
        let mut watch = Watch::default();
        watch.take(reading(8_000, 14_000, 5_000), 5);
        assert_eq!(watch.take(reading(8_000, 14_000, 40), 5), (Level::Calm, 0));
        watch.take(Reading { own_kb: 8_000, free_kb: 14_000, faults: None }, 5);
        assert_eq!(watch.take(reading(8_000, 14_000, 4_000), 5), (Level::Calm, 0), "nor one from none");
    }

    #[test]
    fn three_restarts_running_pause_the_cards_until_resumed() {
        let mut boots = Boots::default();
        boots = boots.started();
        boots = boots.started();
        assert_eq!(boots, Boots { running: 2, paused: false });
        // Staying up forgets them.
        assert_eq!(boots.stayed_up().started(), Boots { running: 1, paused: false });
        boots = boots.started();
        assert_eq!(boots, Boots { running: 3, paused: true });
        assert_eq!(Boots::parse(&boots.text()), boots);
        // Paused, the unit stays up: the pause is kept over later restarts.
        let later = boots.stayed_up().started();
        assert_eq!(later, Boots { running: 1, paused: true });
        assert_eq!(Boots::parse(&later.text()), later);
        assert_eq!(Boots::parse(""), Boots::default());
        assert_eq!(Boots::parse("2\n"), Boots { running: 2, paused: false });
    }
}
