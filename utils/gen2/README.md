# Fledge on the Nest gen 2 (j49): work in progress

This branch makes Fledge's patch and nestlocal's home screen cards run on a
2nd generation Nest Learning Thermostat. It's based on v0.8.0 and has been
tested on one unit: Display-2.8, AM3703, Micron MT29F2G16 NAND, stock
nlclient `67596bb7…`, powered by USB only with no base (`--backplate-sim`).

## Status

| | |
|-|-|
| Patch (nlclient, nestlocal, block cloud, USB shell) | works |
| MQTT and Home Assistant entities | works |
| Fledge tile in the Nest's Settings | works |
| Home screen cards: picture, orientation, ring, button | works |
| Home screen cards: speed and stability | **not usable yet**: see Memory |

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
  mounted turned. Frames stay 480x480 for the UI: each one is scaled to 2/3
  (3x3 to 2x2 weighted) and turned back a quarter on its way to the panel, and
  read back scaled to 3/2. The UI draws on the page that's shown, not the
  other one, because nlclient keeps drawing into its hidden page.
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

## Memory: the open problem

The gen 2 has 58 MB of RAM and no swap. Measured while turning the ring through
the cards:

| | Idle | Using the cards |
|-|-|-|
| nestlocal RSS | 14.5 MB | 22–23 MB |
| Page cache | 21.6 MB | 11.6 MB |
| nlclient RSS | 14.6 MB | 12.0 MB |
| Major page faults | 719 | 1016 |

nlclient's code pages get evicted and re-read from the compressed JFFS2. It
stalls, misses 8 heartbeats, and nlheartbeatd restarts the unit.

Next steps:
1. Keep fewer full frames in memory on the gen 2 (the card cache, the shrunk
   cards).
2. Use lighter transitions on the gen 2 (no zoomed-out chooser).
3. Render at 320 natively instead of 480 then scaling.
4. Leave out what isn't used (Matter, Zigbee) to make nestlocal smaller.

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
