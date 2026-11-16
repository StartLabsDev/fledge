# Testing `gen2-on-main` on a gen 2

This branch is `feature/gen2-cards` moved onto Fledge's current main
(v0.9.0), with the gen 2's memory watched. Nothing in it has run on a gen 2
yet: it was checked against the stock nlclient `67596bb7…` (discovery finds
the same present method, `0x2f56a0`, and vtable slot, `0x4478b8`), by the
unit tests, and by rendering every screen at 320 pixels.

`feature/gen2-cards` is untouched: it is the build to go back to.

## What's new since `feature/gen2-cards`

- **Everything in Fledge v0.8.1 to v0.9.0**: the glance designer, alarm
  cards, air conditioner modes and the rest.
- **The patch is checked harder.** The gen 2's present method is one of the
  reviewed shapes (`abi_shapes.json`), so a gen 2 nlclient of another build
  is reported as unreviewed rather than patched blind.
- **Memory** (`crates/nestlocal/src/health.rs`), on a gen 2 only:
  - Home Assistant gets diagnostic sensors: *Fledgeling memory*, *Memory
    left*, *Nest page faults* (nlclient's major page faults a second, the
    stall that ends in a restart) and *Memory pressure* (calm, tight,
    critical).
  - The cards keep at most 6 frames (3 when tight, 2 when critical), the
    ones asked for longest ago going first.
  - Tight, the UI sleeps twice as long after each frame.
  - Grown past 26 MB, nestlocal stops itself and monit starts it again.
  - After 3 restarts of the unit in a row, none of them staying up 10
    minutes, the cards are paused: *Cards paused* shows a problem in Home
    Assistant, and the *Resume cards* button brings them back.

The thresholds are first guesses. The sensors' history from your unit is
what they'll be set from.

## Known gaps

- **nlclient doesn't say what it shows.** The gen 3's cards are told which
  of nlclient's screens is up (its `UiDisplayChange` report); discovery
  doesn't find that report in the gen 2's nlclient, and the pixel checks
  that stood in for it are off at 320. Going into the Nest's own Settings
  from the Settings card, and coming back, is the part most likely to go
  wrong.
- Glance and Wake to were built on the gen 3's sensors. The near PIR is read
  as someone close; the rest is untried.
- An update installed from Home Assistant is untried on the gen 2's small
  root slot.

## Before you start

Back up first (`fledge backup`). The patch changes, so the unit must be
patched again, not only given a new nestlocal. Since v0.8.1 the app refuses
a gen 1 or 2 unless it is told otherwise:

```sh
export FLEDGE_OLDER_NESTS=1
```

Build and patch as you did before, with the options you used.

## What to try

Please note for each: worked, didn't, or what happened instead.

1. **Patch.** The log should name the build `2019-gen2` and make no
   objection. nlclient starts, the Fledge tile is in Settings, the entities
   reach Home Assistant, the new memory sensors among them.
2. **Cards.** Turn through all your cards for ten minutes, opening a few.
   Watch *Memory pressure* and *Nest page faults* meanwhile.
3. **Many cards.** Give it 10 or more cards and spin through them quickly,
   both ways. Frames should be forgotten and redrawn without the unit
   restarting; *Fledgeling memory* should stay under about 15 MB.
4. **The new screens.** A glance (try two or three layouts), an alarm card
   if you have an alarm, a notification (`fledge.notify`).
5. **A camera card**, opened full screen for a minute. It decodes a JPEG
   every 3 seconds: the heaviest thing the cards do.
6. **Settings.** Open the Settings card, go into the Nest's own Settings,
   press Done. Do the cards come back? (See Known gaps.)
7. **Overnight.** Leave it. In the morning: uptime, and whether *Cards
   paused* is on.
8. **Paused cards**, if you're willing: `echo "3 paused" >
   /media/data/fledge-boots`, restart nestlocal. The cards shouldn't show;
   *Cards paused* should be on; *Resume cards* should bring them back
   within 5 seconds.

## What to send back

- The four memory sensors' history over the tests (a screenshot is fine).
- `/media/log/messages`: the lines starting `health:` and `ui:`.
- If the unit restarts: what you were doing, and the last minute of that
  log from before it.
