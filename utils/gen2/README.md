# Fledge on the Nest gen 2 (j49): work in progress

This branch makes Fledge's patch and nestlocal's home screen cards run on a
2nd generation Nest Learning Thermostat. It's based on v0.8.0 and has been
tested on one unit: Display-2.8, AM3703, Micron MT29F2G16 NAND, stock
nlclient `67596bb7…`, first powered by USB only with no base
(`--backplate-sim`), then on its base with 12 V on the terminals.

## Status

| | |
|-|-|
| Patch (nlclient, nestlocal, block cloud, USB shell) | works |
| MQTT and Home Assistant entities | works |
| Fledge tile in the Nest's Settings | works |
| Home screen cards: picture, orientation, ring, button | works |
| Home screen cards: speed and stability | usable: see Memory |
| Motion, someone close, light, external power | works on a base: see Sensors |

## What changed

- **Screen hand-over** (`fledge-core` discovery and builder, `custom_screen.c`).
  The gen 2 nlclient has no EGL. It presents through one virtual method that
  flips its page and calls `ioctl(FBIOPAN_DISPLAY)`. Discovery finds that
  method by `movw r1, #0x4606` followed by a BL to the ioctl stub, and the
  single vtable slot that points to it (`present_slot`). The patch points the
  slot at `custom_present`, which skips the whole method while the UI owns the
  screen. It also starts the display keeper through NSPR's `PR_CreateThread`,
  because nlclient imports no `pthread_create`.
- **Display** (`nestlocal/src/ui/device.rs`). The panel is 320x320 and not
  mounted turned. The UI draws natively at the panel's side: screens keep
  their 480-unit layout, and a canvas scales units to pixels
  (`paint::set_side`). Frames keep the gen 3's turned layout and are turned
  back a quarter on their way to the panel. The UI draws on the page that's
  shown, not the other one, because nlclient keeps drawing into its hidden
  page.
- **Text.** The gen 2's display is physically smaller too, so on it small
  text is drawn up to a quarter larger (`paint::larger`), fading to none at
  64 units; big numbers are as they are.
- **Input.** The ring is `avago-adbs-a330` (REL_X, clockwise up) and the
  button is `twl4030_pwrbutton` (KEY_POWER). Both are mapped to REL_DIAL and
  KEY_ENTER, and written back as their own codes.
- **Fonts.** The gen 2 has no AkkuratNest-Medium; Bold stands in for it.
- **CPU.** On the gen 2 the UI thread isn't reniced, and it sleeps after each
  frame for as long as the frame took, so nlclient keeps answering
  nlheartbeatd. nestlocal is built at opt-level 3 with NEON, which took a
  frame from about 150 ms to 28 ms.
- **Keeper start.** The keeper starts at nlclient's first bucket commit, which
  can take a long time on the gen 2. Until it runs, the UI merges a field of
  its own (`fledge_keeper`) into the device bucket every 30 s, which makes
  nlclient commit.

- **Someone close** (`nestlocal/src/motion.rs`). The gen 2 has no
  thermopile: its backplate reports a near PIR, which nlclient logs as
  `publishing near pir msg N, threshold T`. That line is read as someone
  close.

## Sensors

The PIR, near PIR and ambient light sensor are read by the backplate's
microcontroller, not by the head: its kernel has no device for them. With
`--backplate-sim` and no base, nlclient gets none of their messages and
Motion, Someone close and the light sensors stay unknown; the simulator's
temperature and humidity are made up.

On its base (12 V on Rh and C, patched without `--backplate-sim`) nlclient
logs the same `publishing pir msg` and `pir …; alv A, alir I` lines as the
gen 3, and `state …, vi …V, vo …V` for the power, so Motion, Someone close,
Light level, Infrared light and External power reach Home Assistant. The
USB port is covered on the base: `--ssh` gives a shell over Wi-Fi.

## Memory and speed

The gen 2 has 58 MB of RAM and no swap. Drawing at 480 and scaling, nestlocal
grew from 8 to 22 MB while turning through the cards. nlclient's code pages
were evicted and re-read from the compressed JFFS2, it missed 8 heartbeats,
and nlheartbeatd restarted the unit.

Drawing natively at 320 more than halves every kept frame (410 KB rather than
921 KB) and removes the scaling pass. Measured on the unit:

| | 480, scaled | 320, native |
|-|-|-|
| nestlocal RSS at start | 8 MB | 6–7 MB |
| nestlocal RSS using the cards | 22–23 MB | 12–14 MB |
| A typical frame | 21–115 ms | about 14 ms (copy 5.5) |
| Uptime while using the cards | reboots within minutes | 3 h+, no reboot |

What's left:
- The thermostat card's colour change (heating to idle) showed single frames
  of 0.5–1.1 s, likely nlclient drawing its own fade unseen at the same time.
  With *Act as a thermostat* off (a panel only, as on this unit) that screen
  is gone.
- nlclient keeps drawing unseen while the UI owns the screen, paced by the
  16.7 ms sleep in `custom_present`. A longer sleep there would leave more
  CPU to the UI.
- Matter and Zigbee could be left out to make nestlocal smaller.

## Working on a unit

Patch with a USB root shell, then iterate on nestlocal without re-patching:

```sh
utils/build-firmware.sh nestlocal
# on the PC: serve the binary
python3 -I utils/gen2/send.py 9997 build/nestlocal-armv7 &
# on the unit, through its USB shell (/dev/ttyACM0)
python3 -I utils/gen2/nest.py 'stty -echo'
python3 -I utils/gen2/nest.py 'nc <pc-ip> 9997 > /nestlabs/sbin/nestlocal.new; cd /nestlabs/sbin && mv nestlocal.new nestlocal && chmod 755 nestlocal && sync && kill $(pidof nestlocal)' 90
```

monit restarts nestlocal. Logs go to `/media/log/messages`. The UI logs
frame timings as `ui: N frames, … ms each (…)`. `recv.py` takes a file from
the unit (`… | nc <pc-ip> <port>`), for example a framebuffer page:
`dd if=/dev/fb0 bs=1280 skip=<pan row> count=320 | gzip -1`.

The root slot has little room. Keep an eye on `df -k /` before copying a
binary.
