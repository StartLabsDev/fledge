//! The unit's display, ring and button, for the UI.
//!
//! Display: /dev/fb0, two 480x640 pages of 32-bit BGRX that nlclient flips
//! between; the visible one is the page the panning offset names. While the
//! UI owns the display, nlclient's patched eglSwapBuffers (custom_swap in
//! custom_screen.c) presents nothing, so the UI paints the visible page.
//! nlclient sees ownership through OWNER: this process's pid, checked live,
//! so the display returns to nlclient the moment this process is gone.
//!
//! Input: the ring (REL_DIAL on "2280000.adc") and the button (KEY_ENTER on
//! "gpio-keys"). Grabbed (EVIOCGRAB), only this process reads them. Events
//! written to a node go to its readers, or only to the grabber while it is
//! grabbed: `nudge` lets go for the length of one write to reach nlclient.
//!
//! The gen 2: its display is 320x320 (two pages of 320x640), each frame
//! scaled by 2/3 on its way to the panel and by 3/2 read back, so the UI
//! draws at 480 as on the gen 3. Its ring is an optical sensor ("avago-adbs-
//! a330", REL_X, clockwise up, about 4000 counts a turn) and its button the
//! power button ("twl4030_pwrbutton", KEY_POWER): read as REL_DIAL and
//! KEY_ENTER, and written back to nlclient as its own.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use super::prof;

pub const OWNER: &str = "/var/run/unfr0zen/screen.owner";
const REST: &str = "/var/run/unfr0zen/screen.rest";
const KEEPER_RESTS: &str = "/var/run/unfr0zen/keeper.rests";
const KEEPER: &str = "/var/run/unfr0zen/keeper.pid";
const BEEPER: &str = "pwm-beeper";
const EV_SND: u16 = 0x12;
const SND_TONE: u16 = 2;
pub const EV_KEY: u16 = 1;
pub const EV_REL: u16 = 2;
pub const KEY_ENTER: u16 = 28;
pub const REL_DIAL: u16 = 7;
const REL_X: u16 = 0;
const KEY_POWER: u16 = 116;

/// A generation's ring and button: their input devices, and the codes they
/// report. `ring_sign` turns its counts to the gen 3's (anticlockwise up).
struct Controls {
    ring: &'static str,
    ring_code: u16,
    ring_sign: i32,
    button: &'static str,
    button_code: u16,
}

const CONTROLS: [Controls; 2] = [
    Controls { ring: "2280000.adc", ring_code: REL_DIAL, ring_sign: 1, button: "gpio-keys", button_code: KEY_ENTER },
    Controls { ring: "avago-adbs-a330", ring_code: REL_X, ring_sign: 1, button: "twl4030_pwrbutton", button_code: KEY_POWER },
];
const EVIOCGRAB: libc::c_ulong = 0x4004_4590;

pub fn own(yes: bool) {
    let result = if yes {
        crate::settings_ui::publish("screen.owner", format!("{} 0", std::process::id()).as_bytes())
    } else {
        match std::fs::remove_file(OWNER) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    };
    if let Err(e) = result {
        crate::logf!("ui: {OWNER}: {e}");
    }
}

/// The owner's heartbeat: nlclient's patch (keeper.rests and on) takes the
/// display back from an owner whose beat stops changing.
pub fn beat(beat: u32) {
    if let Err(e) = crate::settings_ui::publish("screen.owner", format!("{} {beat}", std::process::id()).as_bytes()) {
        crate::logf!("ui: {OWNER}: {e}");
    }
}

/// Favours the calling thread (the UI's) over the rest of the unit's work
/// when both want its one core: a frame late shows as a stutter, and what
/// it holds up meanwhile (MQTT, nlclient's own housekeeping) is in no hurry.
/// It sleeps between frames and while nothing moves.
///
/// Not on the gen 2, whose frames take most of its slower core: nlclient
/// held up there misses its heartbeats, and nlheartbeatd restarts the unit.
pub fn favour() {
    if crate::device::current().generation == 2 {
        return;
    }
    // SAFETY: setpriority on the calling thread (0), which Linux takes a
    // nice value for apart from the process's other threads.
    #[cfg(target_os = "linux")]
    if unsafe { libc::setpriority(libc::PRIO_PROCESS as _, 0, -10) } != 0 {
        crate::logf!("ui: priority: {}", std::io::Error::last_os_error());
    }
}

/// Whether nlclient's patch can leave the display to go out while the UI
/// holds it (screen.rest), and takes it back from a hung owner.
pub fn rests() -> bool {
    Path::new(KEEPER_RESTS).exists()
}

/// Leaves the display to nlclient's own timing (true) though the UI holds
/// it, or has nlclient keep it lit while the UI owns it.
pub fn rest(yes: bool) {
    let result = if yes {
        crate::settings_ui::publish("screen.rest", b"")
    } else {
        match std::fs::remove_file(REST) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    };
    if let Err(e) = result {
        crate::logf!("ui: {REST}: {e}");
    }
}

/// Whether the patch's display keeper runs in the nlclient running now: it
/// starts at nlclient's first bucket commit, which on the gen 2 can be long
/// in coming (Sender::nudge_nlclient).
pub fn keeper_runs() -> bool {
    let Ok(pid) = std::fs::read_to_string(KEEPER) else { return false };
    std::fs::read_to_string(format!("/proc/{}/comm", pid.trim())).is_ok_and(|comm| comm.trim() == "nlclient")
}

/// Asks nlclient to light its display (custom_screen.c's keeper).
pub fn wake_display() {
    if let Err(e) = crate::settings_ui::publish("display.wake", b"") {
        crate::logf!("ui: display.wake: {e}");
    }
}

/// The Nest's click: the beeper's tone, at nlclient's own clickfrequency
/// and clickduration (/nestlabs/etc/client.config).
pub struct Beeper(Option<File>);

impl Beeper {
    pub fn open() -> Beeper {
        Beeper(node(BEEPER).map_err(|e| crate::logf!("ui: {e}: no clicks")).ok())
    }

    /// A notification's chime: two short rising notes.
    pub fn chime(&mut self) {
        let Some(file) = &mut self.0 else { return };
        for (tone, ms) in [(1568, 90), (0, 40), (2093, 150)] {
            let _ = file.write_all(&[encode(EV_SND, SND_TONE, tone), encode(0, 0, 0)].concat());
            std::thread::sleep(Duration::from_millis(ms));
        }
        let _ = file.write_all(&[encode(EV_SND, SND_TONE, 0), encode(0, 0, 0)].concat());
    }

    pub fn click(&mut self) {
        self.tick(2000, 3);
    }

    /// A softer, lower click: a light turned off by the ring.
    pub fn click_low(&mut self) {
        self.tick(900, 5);
    }

    fn tick(&mut self, hertz: i32, ms: u64) {
        let Some(file) = &mut self.0 else { return };
        let _ = file.write_all(&[encode(EV_SND, SND_TONE, hertz), encode(0, 0, 0)].concat());
        std::thread::sleep(Duration::from_millis(ms));
        let _ = file.write_all(&[encode(EV_SND, SND_TONE, 0), encode(0, 0, 0)].concat());
    }
}

/// Switches the display's backlight on or off (bl_power), leaving its
/// brightness, which nlclient sets and lit() reads, as it is: off, a lit
/// display is truly dark, where black pixels still glow.
pub fn backlight(on: bool) {
    let Ok(dir) = std::fs::read_dir("/sys/class/backlight") else { return };
    for entry in dir.flatten() {
        // FB_BLANK_UNBLANK, FB_BLANK_POWERDOWN.
        if let Err(e) = std::fs::write(entry.path().join("bl_power"), if on { "0" } else { "4" }) {
            crate::logf!("ui: backlight {}: {e}", entry.path().display());
        }
    }
}

/// Whether the display is lit.
pub fn lit() -> bool {
    let Ok(dir) = std::fs::read_dir("/sys/class/backlight") else { return true };
    dir.flatten().any(|entry| {
        std::fs::read_to_string(entry.path().join("brightness"))
            .ok().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(1) > 0
    })
}

pub struct Display {
    file: File,
    map: *mut u8,
    len: usize,
    stride: usize,
    /// The panel's side: SIDE, or SMALL (the gen 2).
    side: usize,
    /// While the UI owns the display, it draws into the page that isn't
    /// shown and then pans to it, so no frame shows half drawn.
    flip: Option<Flip>,
    /// The UI's last frame, as it went to the display: a transition starts
    /// from it, as reading the display back is slow (its memory isn't cached).
    shown: Frame,
}

/// The two pages, as nlclient's EGL flips between them: which one shows,
/// which one nlclient showed (its own again when the UI is done), and the
/// rows each lacks of the newest frame.
struct Flip {
    pages: [usize; 2],
    front: usize,
    home: usize,
    stale: [Option<(usize, usize)>; 2],
}

const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
const CAPTURE_REQUEST: &str = "/var/run/unfr0zen/capture.request";
const FBIOPAN_DISPLAY: libc::c_ulong = 0x4606;

fn union(a: Option<(usize, usize)>, b: Option<(usize, usize)>) -> Option<(usize, usize)> {
    match (a, b) {
        (Some((a0, a1)), Some((b0, b1))) => Some((a0.min(b0), a1.max(b1))),
        (a, None) => a,
        (None, b) => b,
    }
}

// SAFETY: the mapping is only touched by the UI thread that owns Display.
unsafe impl Send for Display {}

fn sys(name: &str) -> Result<String, String> {
    let path = format!("/sys/class/graphics/fb0/{name}");
    std::fs::read_to_string(&path).map(|s| s.trim().to_owned()).map_err(|e| format!("{path}: {e}"))
}

impl Display {
    pub fn open() -> Result<Display, String> {
        let stride: usize = sys("stride")?.parse().map_err(|_| "fb0: stride".to_string())?;
        let size = sys("virtual_size")?;
        let rows: usize = size.split(',').nth(1).and_then(|s| s.parse().ok()).ok_or("fb0: virtual_size")?;
        let side: usize = size.split(',').next().and_then(|s| s.parse().ok()).ok_or("fb0: virtual_size")?;
        if side != SIDE && side != SMALL {
            return Err(format!("fb0: a {side} pixel display"));
        }
        let file = std::fs::OpenOptions::new().read(true).write(true).open("/dev/fb0")
            .map_err(|e| format!("/dev/fb0: {e}"))?;
        let len = stride * rows;
        // SAFETY: a shared mapping of the framebuffer device, len its size.
        let map = unsafe {
            libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED,
                       file.as_raw_fd(), 0)
        };
        if map == libc::MAP_FAILED {
            return Err(format!("/dev/fb0: mmap: {}", std::io::Error::last_os_error()));
        }
        Ok(Display { file, map: map.cast(), len, stride, side, flip: None, shown: vec![0; SIDE * ROW] })
    }

    /// Writes the frame's rows `first` to `last` into a page: as they are,
    /// or on the gen 2 the panel's columns they make, scaled by 2/3 and
    /// turned back a quarter, its panel showing the framebuffer unturned.
    fn put(page: &mut [u8], stride: usize, side: usize, frame: &[u8], first: usize, last: usize) {
        if side == SIDE {
            for y in first..=last.min(SIDE - 1) {
                page[y * stride..y * stride + ROW].copy_from_slice(&frame[y * ROW..(y + 1) * ROW]);
            }
            return;
        }
        let (from, to) = (first * 2 / 3, (last * 2 / 3 + 1).min(SMALL - 1));
        let mut line = [0u8; SMALL * 4];
        for y in 0..SMALL {
            panel_row(frame, y, from, to, &mut line);
            page[y * stride + from * 4..y * stride + (to + 1) * 4].copy_from_slice(&line[from * 4..(to + 1) * 4]);
        }
    }

    /// A page as a Frame: the gen 2's scaled by 3/2 and turned a quarter,
    /// from a copy read in whole rows (its memory is slow a pixel at a time).
    fn take(page: &[u8], stride: usize, side: usize) -> Frame {
        let mut frame = Vec::with_capacity(SIDE * ROW);
        if side == SIDE {
            for y in 0..SIDE {
                frame.extend_from_slice(&page[y * stride..y * stride + ROW]);
            }
            return frame;
        }
        let mut panel = vec![0u8; SMALL * SMALL * 4];
        for y in 0..SMALL {
            panel[y * SMALL * 4..(y + 1) * SMALL * 4].copy_from_slice(&page[y * stride..y * stride + SMALL * 4]);
        }
        for y in 0..SIDE {
            let column = y * 2 / 3 * 4;
            for x in 0..SIDE {
                let at = (SIDE - 1 - x) * 2 / 3 * SMALL * 4 + column;
                frame.extend_from_slice(&panel[at..at + 4]);
            }
        }
        frame
    }

    /// The panning offset's row: the page shown.
    fn pan(&self) -> usize {
        sys("pan").ok().and_then(|s| s.split(',').nth(1)?.parse().ok()).unwrap_or(0)
    }

    /// The first byte of the visible page.
    fn page(&self) -> usize {
        match &self.flip {
            Some(flip) => flip.pages[flip.front] * self.stride,
            None => (self.pan() * self.stride).min(self.len.saturating_sub(self.stride * self.side)),
        }
    }

    /// Pans the display to the page starting at row `y`.
    fn pan_to(&mut self, y: usize) -> std::io::Result<()> {
        let mut var = [0u32; 40];
        // SAFETY: fb_var_screeninfo is 160 bytes; yoffset its sixth word.
        unsafe {
            if libc::ioctl(self.file.as_raw_fd(), FBIOGET_VSCREENINFO as _, var.as_mut_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            var[5] = y as u32;
            if libc::ioctl(self.file.as_raw_fd(), FBIOPAN_DISPLAY as _, var.as_mut_ptr()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Whether the display shows a page other than the UI's own last frame:
    /// waking can pan it back to nlclient's.
    pub fn lost(&self) -> bool {
        match &self.flip {
            Some(flip) => self.pan() != flip.pages[flip.front],
            None => false,
        }
    }

    /// The UI takes the display: from now on frames are drawn unseen and
    /// flipped to. Without two pages it draws straight on the shown one.
    pub fn begin(&mut self) {
        let rows = self.len / self.stride;
        if rows < 2 * self.side {
            return;
        }
        // The gen 2's nlclient goes on drawing into the page it doesn't
        // show (its present is skipped, not its drawing): the UI keeps to the
        // one shown, drawing straight on it.
        let pages = if self.side == SIDE { [0, rows / 2] } else { [self.pan(); 2] };
        let home = usize::from(self.side == SIDE && self.pan() >= rows / 2);
        self.flip = Some(Flip { pages, front: home, home, stale: [Some((0, SIDE - 1)); 2] });
    }

    /// Gives the display back showing `frame` (nlclient's) on nlclient's
    /// own page, as its EGL left it.
    pub fn end(&mut self, frame: &[u8]) {
        let Some(flip) = self.flip.take() else { return self.show(frame) };
        let start = flip.pages[flip.home] * self.stride;
        let (stride, side) = (self.stride, self.side);
        Self::put(&mut self.bytes()[start..], stride, side, frame, 0, SIDE - 1);
        if let Err(e) = self.pan_to(flip.pages[flip.home]) {
            crate::logf!("ui: pan: {e}");
        }
    }

    /// Writes rows into the page to show next and flips to it: `rows`
    /// those changed (None, all), each given by `each`.
    fn present(&mut self, rows: Option<(usize, usize)>, mut each: impl FnMut(usize, &mut [u8])) {
        let (stride, side) = (self.stride, self.side);
        let Some(flip) = self.flip.as_mut() else {
            let start = self.page();
            let (first, last) = rows.unwrap_or((0, SIDE - 1));
            if side != SIDE {
                for y in first..=last.min(SIDE - 1) {
                    each(y, &mut self.shown[y * ROW..(y + 1) * ROW]);
                }
                // SAFETY: map is len bytes, mapped for this Display's life; shown is a separate field.
                let page = unsafe { std::slice::from_raw_parts_mut(self.map.add(start), self.len - start) };
                return Self::put(page, stride, side, &self.shown, first, last);
            }
            let page = &mut self.bytes()[start..];
            for y in first..=last.min(SIDE - 1) {
                each(y, &mut page[y * stride..y * stride + ROW]);
            }
            return;
        };
        let back = 1 - flip.front;
        let all = Some((0, SIDE - 1));
        let changed = if rows.is_none() { all } else { rows };
        let (first, last) = union(changed, flip.stale[back]).unwrap_or((0, SIDE - 1));
        flip.stale[back] = None;
        flip.stale[flip.front] = union(flip.stale[flip.front], changed);
        flip.front = back;
        let y = flip.pages[back];
        let start = y * stride;
        // SAFETY: map is len bytes, mapped for this Display's life; shown is a separate field.
        let page = unsafe { std::slice::from_raw_parts_mut(self.map.add(start), self.len - start) };
        let shown = &mut self.shown;
        prof::time(prof::Part::Copy, || {
            for row in first..=last.min(SIDE - 1) {
                let line = &mut shown[row * ROW..(row + 1) * ROW];
                each(row, line);
                if side == SIDE {
                    page[row * stride..row * stride + ROW].copy_from_slice(line);
                }
            }
            if side != SIDE {
                Self::put(page, stride, side, shown, first, last);
            }
        });
        // For tools (utils/nest-screen.py): the frame about to show, whole,
        // where reading the framebuffer could catch the next one being drawn.
        if std::fs::remove_file(CAPTURE_REQUEST).is_ok() {
            let frame = if side == SIDE {
                (0..SIDE).flat_map(|row| page[row * stride..row * stride + ROW].to_vec()).collect()
            } else {
                shown.clone()
            };
            let _ = crate::settings_ui::publish("capture.raw", &frame);
        }
        if let Err(e) = prof::time(prof::Part::Pan, || self.pan_to(y)) {
            crate::logf!("ui: pan: {e}");
        }
    }

    fn bytes(&mut self) -> &mut [u8] {
        // SAFETY: map is len bytes, mapped for this Display's life.
        unsafe { std::slice::from_raw_parts_mut(self.map, self.len) }
    }

    /// The visible page's top 480 rows as a Frame: what nlclient shows.
    pub fn save(&mut self) -> Frame {
        let (start, stride, side) = (self.page(), self.stride, self.side);
        let page = &self.bytes()[start..];
        prof::time(prof::Part::Read, || Self::take(page, stride, side))
    }

    /// What shows: the UI's own last frame while it has the display (and
    /// it wasn't panned away), else the display read back.
    pub fn current(&mut self) -> Frame {
        if self.flip.is_some() && !self.lost() {
            return self.shown.clone();
        }
        self.save()
    }

    /// Shows a Frame.
    pub fn show(&mut self, frame: &[u8]) {
        self.present(None, |y, row| row.copy_from_slice(&frame[y * ROW..(y + 1) * ROW]));
    }

    /// Shows a Frame of which only rows `first` to `last` changed.
    pub fn show_rows(&mut self, frame: &[u8], first: usize, last: usize) {
        self.present(Some((first, last)), |y, row| row.copy_from_slice(&frame[y * ROW..(y + 1) * ROW]));
    }

    /// `from` sliding out and `to` in by `by` pixels of 480, sideways as the
    /// display stands: the framebuffer's rows, as it is mounted turned.
    pub fn slide(&mut self, from: &[u8], to: &[u8], by: usize, clockwise: bool) {
        let by = by.min(SIDE);
        self.present(None, |y, row| {
            let (source, at) = if clockwise {
                if y + by < SIDE { (from, y + by) } else { (to, y + by - SIDE) }
            } else if y >= by { (from, y - by) } else { (to, y + SIDE - by) };
            row.copy_from_slice(&source[at * ROW..(at + 1) * ROW]);
        });
    }

    /// `to` over `from` at `alpha` of 256.
    pub fn fade(&mut self, from: &[u8], to: &[u8], alpha: u16) {
        let alpha = alpha.min(256) as u32;
        let mut line = [0u8; ROW];
        self.present(None, |y, row| {
            let (a, b) = (&from[y * ROW..(y + 1) * ROW], &to[y * ROW..(y + 1) * ROW]);
            // A pixel at a time, two channels to a multiply (blue and red,
            // then green and the unused byte), into a line written to the
            // framebuffer whole: its memory is slow a byte at a time.
            for ((out, a), b) in line.as_chunks_mut::<4>().0.iter_mut().zip(a.as_chunks::<4>().0).zip(b.as_chunks::<4>().0) {
                let (a, b) = (u32::from_le_bytes(*a), u32::from_le_bytes(*b));
                let even = ((a & 0x00ff_00ff) * (256 - alpha) + (b & 0x00ff_00ff) * alpha) >> 8 & 0x00ff_00ff;
                let odd = (((a >> 8) & 0x00ff_00ff) * (256 - alpha) + ((b >> 8) & 0x00ff_00ff) * alpha) & 0xff00_ff00;
                *out = (even | odd).to_le_bytes();
            }
            row.copy_from_slice(&line);
        });
    }

}

/// The display's side, and a framebuffer row of it in bytes.
const SIDE: usize = 480;
const ROW: usize = SIDE * 4;
/// The gen 2's display side: 2/3 of SIDE.
const SMALL: usize = 320;

/// Of the three frame rows or columns a gen 2 pixel's two come from, the
/// nearer and the middle one.
fn thirds(at: usize) -> (usize, usize) {
    (3 * (at / 2) + at % 2 * 2, 3 * (at / 2) + 1)
}

/// The gen 2 panel's row `y`, pixels `from` to `to`, from a Frame: the Frame
/// turned back a quarter (the panel's row y is its column SMALL - 1 - y),
/// each 3x3 pixels to 2x2, weighted 2:1 towards the nearer of the three.
fn panel_row(frame: &[u8], y: usize, from: usize, to: usize, out: &mut [u8; SMALL * 4]) {
    let (cn, cf) = thirds(SMALL - 1 - y);
    let (cn, cf) = (cn * 4, cf * 4);
    for x in from..=to {
        let (rn, rf) = thirds(x);
        let (a, b) = (&frame[rn * ROW..(rn + 1) * ROW], &frame[rf * ROW..(rf + 1) * ROW]);
        for c in 0..4 {
            let v = 4 * a[cn + c] as u32 + 2 * (a[cf + c] as u32 + b[cn + c] as u32) + b[cf + c] as u32;
            out[x * 4 + c] = (v / 9) as u8;
        }
    }
}

/// A screen as the framebuffer holds it: 480 rows of 480 BGRX pixels, the
/// picture turned a quarter anticlockwise, as the panel shows the
/// framebuffer turned a quarter clockwise.
pub type Frame = Vec<u8>;

/// A screen drawn for the panel (paint::for_panel) as a Frame: as it is.
pub fn frame(canvas: super::paint::Canvas) -> Frame {
    canvas.pixmap.take()
}

impl Drop for Display {
    fn drop(&mut self) {
        // SAFETY: unmaps what open mapped.
        unsafe { libc::munmap(self.map.cast(), self.len) };
    }
}

pub struct Event {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
}

pub struct Input {
    ring: File,
    button: File,
    grabbed: bool,
    controls: &'static Controls,
}

fn node(name: &str) -> Result<File, String> {
    let dir = std::fs::read_dir("/sys/class/input").map_err(|e| format!("/sys/class/input: {e}"))?;
    for entry in dir.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if !file.starts_with("event") {
            continue;
        }
        let device = std::fs::read_to_string(entry.path().join("device/name")).unwrap_or_default();
        if device.trim() == name {
            let path = Path::new("/dev/input").join(&file);
            return std::fs::OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK)
                .open(&path).map_err(|e| format!("{}: {e}", path.display()));
        }
    }
    Err(format!("no input device {name}"))
}

use std::os::unix::fs::OpenOptionsExt;

fn grab(file: &File, yes: bool) -> std::io::Result<()> {
    // SAFETY: EVIOCGRAB takes an int by value.
    if unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGRAB as _, yes as libc::c_int) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// One input_event as the unit's 32-bit kernel lays it out.
fn encode(kind: u16, code: u16, value: i32) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[8..10].copy_from_slice(&kind.to_ne_bytes());
    out[10..12].copy_from_slice(&code.to_ne_bytes());
    out[12..16].copy_from_slice(&value.to_ne_bytes());
    out
}

impl Input {
    pub fn open() -> Result<Input, String> {
        let mut missing = Vec::new();
        for controls in &CONTROLS {
            match (node(controls.ring), node(controls.button)) {
                (Ok(ring), Ok(button)) => return Ok(Input { ring, button, grabbed: false, controls }),
                (Err(e), _) | (_, Err(e)) => missing.push(e),
            }
        }
        Err(missing.join(", "))
    }

    pub fn grab(&mut self, yes: bool) {
        if self.grabbed == yes {
            return;
        }
        for file in [&self.ring, &self.button] {
            if let Err(e) = grab(file, yes) {
                crate::logf!("ui: grab {yes}: {e}");
            }
        }
        self.grabbed = yes;
    }

    /// Waits up to `timeout` for input; returns what arrived.
    pub fn wait(&mut self, timeout: Duration) -> Vec<Event> {
        let mut fds = [
            libc::pollfd { fd: self.ring.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: self.button.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: fds is two valid pollfds.
        unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout.as_millis() as libc::c_int) };
        let mut events = Vec::new();
        for file in [&mut self.ring, &mut self.button] {
            let mut buffer = [0u8; 16 * 32];
            while let Ok(read) = file.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                for raw in buffer[..read].as_chunks::<16>().0 {
                    let kind = u16::from_ne_bytes([raw[8], raw[9]]);
                    let code = u16::from_ne_bytes([raw[10], raw[11]]);
                    let value = i32::from_ne_bytes([raw[12], raw[13], raw[14], raw[15]]);
                    let c = self.controls;
                    match (kind, code) {
                        (EV_REL, code) if code == c.ring_code => {
                            events.push(Event { kind, code: REL_DIAL, value: value * c.ring_sign });
                        }
                        (EV_KEY, code) if code == c.button_code => events.push(Event { kind, code: KEY_ENTER, value }),
                        (EV_REL | EV_KEY, _) => events.push(Event { kind, code, value }),
                        _ => {}
                    }
                }
            }
        }
        events
    }

    /// A turn of the ring for nlclient, as a hand makes one: `reports`
    /// reports of `raw` counts, 25 ms apart (its menus take no other).
    pub fn hand_turn(&mut self, raw: i32, reports: usize) {
        let c = self.controls;
        let report: Vec<u8> = [encode(EV_REL, c.ring_code, raw * c.ring_sign), encode(0, 0, 0)].concat();
        for _ in 0..reports {
            self.send(true, &report);
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// A press and release of the button, for nlclient.
    pub fn press(&mut self) {
        let key = self.controls.button_code;
        let down: Vec<u8> = [encode(EV_KEY, key, 1), encode(0, 0, 0)].concat();
        let up: Vec<u8> = [encode(EV_KEY, key, 0), encode(0, 0, 0)].concat();
        self.send(false, &down);
        std::thread::sleep(Duration::from_millis(120));
        self.send(false, &up);
    }

    fn send(&mut self, ring: bool, report: &[u8]) {
        let was = self.grabbed;
        self.grab(false);
        let file = if ring { &mut self.ring } else { &mut self.button };
        if let Err(e) = file.write_all(report) {
            crate::logf!("ui: inject: {e}");
        }
        // The write reached this process's reader too: it is not input.
        let mut discard = [0u8; 16 * 8];
        while matches!(file.read(&mut discard), Ok(n) if n > 0) {}
        self.grab(was);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gen_2_panel_shows_a_frame_at_two_thirds() {
        // A frame of 3x3 blocks, each one value: each comes out a 2x2 block.
        let mut frame = vec![0u8; SIDE * ROW];
        for y in 0..SIDE {
            for x in 0..SIDE {
                let v = ((x / 3 + y / 3) % 2 * 200) as u8;
                frame[y * ROW + x * 4..y * ROW + x * 4 + 4].copy_from_slice(&[v, v, v, 0]);
            }
        }
        let mut line = [0u8; SMALL * 4];
        for y in 0..SMALL {
            panel_row(&frame, y, 0, SMALL - 1, &mut line);
            for x in 0..SMALL {
                // Turned back a quarter: the panel's (x, y) is the frame's (319 - y, x).
                let want = (((SMALL - 1 - y) / 2 + x / 2) % 2 * 200) as u8;
                assert_eq!(line[x * 4..x * 4 + 4], [want, want, want, 0], "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_gen_2_page_reads_back_at_480() {
        let mut page = vec![0u8; SMALL * SMALL * 4];
        page[(10 * SMALL + 20) * 4] = 9;
        let frame = Display::take(&page, SMALL * 4, SMALL);
        assert_eq!(frame.len(), SIDE * ROW);
        assert_eq!(frame[(30 * SIDE + 464) * 4], 9);
    }
}
