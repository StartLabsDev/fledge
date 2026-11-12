//! Doom, chosen on the UNFR0ZEN screen: nlclient cannot start a program, so
//! it leaves doom.request and this runs the launcher. The launcher takes the
//! display through nlclient's screen hand-over (screen.owner), so nlclient
//! keeps running and controlling the heating while the game plays; an
//! nlclient patched without the hand-over is stopped and started again. The
//! home screen cards stand aside meanwhile (playing). The launcher has a
//! session of its own, so a restart of nestlocal leaves the game running.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::{logf, settings_ui};

const LAUNCHER: &str = "/nestlabs/sbin/unfr0zen-doom";
static PLAYING: AtomicBool = AtomicBool::new(false);

/// Whether the game was launched and hasn't ended: the display, the ring and
/// the button are its own.
pub fn playing() -> bool {
    PLAYING.load(Ordering::Relaxed)
}

pub fn start() {
    let spawned = std::thread::Builder::new().name("doom".into()).spawn(|| loop {
        std::thread::sleep(Duration::from_millis(500));
        if !settings_ui::take_request("doom") {
            continue;
        }
        logf!("doom: launching");
        // The cards let go of the ring and button before the game grabs them.
        PLAYING.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(600));
        match Command::new("setsid").arg(LAUNCHER)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status() {
            Ok(status) => logf!("doom: over ({status})"),
            Err(e) => logf!("doom: {LAUNCHER}: {e}"),
        }
        // A press made while the game ran is not a new request.
        settings_ui::take_request("doom");
        PLAYING.store(false, Ordering::Relaxed);
    });
    if let Err(e) = spawned {
        logf!("doom: {e}");
    }
}
