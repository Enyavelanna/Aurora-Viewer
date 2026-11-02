# AuroraC2: Combat 2.0 for Aurora Viewer

A private fork of [Aurora Viewer](https://github.com/Aurora-Viewer/Aurora-Viewer), the Second Life viewer written in Rust with a Vulkan renderer. This fork adds **Linden Combat 2.0** tools on the `feat/combat-2` branch (the default branch of this repository). Everything else is Aurora Viewer as it is upstream.

> **Status: pre-release, not validated on a grid.** The features below are covered by unit tests and offline demos. None of them has been tested against live Combat 2.0 events yet.
> **Every combat feature is off by default.** With nothing turned on, the viewer behaves like Aurora Viewer.

This is not an official Aurora Viewer release, and it is not provided or supported by Linden Lab. For the full upstream project (download, features, build, contributing), see the upstream README: <https://github.com/Aurora-Viewer/Aurora-Viewer#readme>.

## Getting a build

Download the latest zip from the [Releases](../../releases) page, unzip it anywhere and run `aurora-viewer.exe` (Windows x64, portable). Check the zip against the published SHA-256 file. Do not let the in-app updater replace this build unless you want to go back to the official one.

## Where everything is

Everything lives in one native window, opened by the **Combat** toolbar button (shield icon; it is in the default toolbar, and a customised toolbar can add it from the toolbar editor) or from **Me > Advanced > Combat**. Every tab is a dashboard of cards, and a card that differs from its defaults shows a reset button. Nothing is added to the other Preferences pages. The master switch **Combat mode** suspends every feature without losing your settings. Interface text is available in English, French and Japanese.

## What it adds

### Combat events
- Reads the official Combat 2.0 **DEATH** events through the Aurora bridge helper (helper 1.42, version-2 JSON envelope).
- Each packet is checked for sequence, duplicates, size and the helper's identity. Gaps and overflows are counted and reported ("N event(s) could not be received; streaks are now uncertain").
- Events are classified: **kill**, **death**, **suicide**, **environmental death** or **observed**.
- **DAMAGE events are deliberately ignored.** They never play a sound, marker or award.

### Kill feedback
- **Kill marker**: a short cross at the centre of the screen on your own confirmed kill (size, colour, opacity, duration).
- **Kill sound**: an optional sound (any asset UUID) with its own volume.
- **Kill feed**: an on-screen list of recent eliminations (position, size, opacity, lifetime, count).

### Awards, announcements and medals
- **Multikills** within 5 seconds: First, Double, Triple, Multi, Ultra.
- **Kill streaks** without dying: 5, 10, 15, 20, 30.
- **Rivalry**: Revenge, Dominating, Comeback.
- One optional announcement sound per award, and an animated **medal banner** with an optional image.
- An **award studio** to set the sound and image of every award, read its trigger rule and preview it.

### Combat log
- A **Combat tab** in Conversations (can be hidden) with All and Mine views.
- The whole session is kept in memory, with an optional background writer that saves a JSONL file per session. **Clear log** moves the file to a recoverable `trash` folder.

### Tactical pings
- **Hold-to-ping wheel** with four sectors (Contact, Objective, Help / Regroup, Ping). A tap places a plain ping. Works in mouselook and out of it.
- The ping lands on the nearest surface or terrain along your aim, ignoring your own avatar and worn equipment.
- Others can acknowledge ("On my way") and you can cancel your ping. A three-second cooldown limits spam.
- **Off-screen guidance**: pings outside the view are drawn around the screen edge, grouped by bearing.

### Combat minimap and region annotations
- Persistent **markers and zones** on the minimap (circles, rectangles, lines, freehand paths), with colour, altitude, labels and distances.
- A **region marker editor** to build callout maps: pins with letters and icons, round, rectangular (rotatable) and polygon zones, arrows, routes, free text and freehand strokes, each with its own colour, fill, line width and dashes. A tool rail with shortcuts (V select, P pin, O circle, R rectangle, G polygon, A arrow, L route, F freehand, T text, M ruler, E eraser), zoom about the pointer, box and multi-selection, handles to reshape, copy and paste, alignment, layers that hide or lock a group, presets (Spawn, Objective, Danger, Cover, Attack path, Callout), a ruler, a scale bar, grid and marker snapping, and 200 steps of undo and redo. The Layers tab lists every marker like a layer panel (eye, lock, rename, bring forward or send backward), markers have no default name, a name can be dragged where it reads best, and a click with the circle tool places a point. A square only comes from holding Shift while dragging a corner.
- The minimap edits the same markers in place: with edit mode on, click or box to select, drag to move, pull a handle to reshape, Delete, Ctrl+Z and Ctrl+Y, a double click opens the region editor, and the right-click menu places a pin or a point, renames, recolours, duplicates or deletes.
- Annotation sets are saved per region and grid, with their layers.
- Optional minimap rotation with your avatar and custom player dots.

### Sharing
- Share markers or whole sets with chosen residents, selected people or the group members in the region, as bounded multi-part transfers.
- Incoming messages are authenticated to their sender and protected against replays. Sender and receiver can each be turned off.
- A marker set has **one format** (format 4: its name, its layers and its markers), the same in a file and in what is sent to residents. Older transfers and single-marker shares are no longer understood: everyone sharing needs the same build.
- **Set file:** the editor's Set tab exports the active set to a JSON file and imports one as a new set of the region (the file is checked: format, sizes, layers and every marker).
- **Picture of the map:** the same tab saves the annotated region as a PNG (1024, 2048 or 4096 pixels): height-shaded terrain, optional objects and parcel lines, every marker (dashes, hatching, arrowheads, pins, texts, names where you put them), a title with the region and set names, a legend of the named pins, the scale and the north.

### Zoomed aim sensitivity
- In mouselook, while the right-button zoom is held, the turn rate can follow the zoom so a scoped view moves across the screen as fast as an unzoomed one.
- **Me > Advanced > Combat > Feedback > Zoomed aim sensitivity**, from 0 to 1. **0 (default) keeps the plain turn rate**, as the Legacy Viewer does. It never speeds you up on a wider-than-normal view and only applies while Combat mode is on.

## Try it without a grid

Offline demos run without logging in: set `AURORA_DEMO_COMBAT` to `1`, `pings`, `ping-wheel`, `navigation` or `workspace`. See [docs/TESTING.md](docs/TESTING.md).

## Known limits

- Not validated on a grid: helper 1.42 compilation, real DEATH events, award thresholds, real medal audio, the JSONL journal, and two-client sharing and acknowledgement.
- Standings and awards depend on the helper's relay. Missing events make streaks uncertain, and the viewer says so.
- The feel of zoomed aim sensitivity above 0 has not been tuned in play.

## Keeping up with upstream

This branch is kept up to date by merging Aurora Viewer `main`. The upstream README is replaced here, so a merge conflict on `README.md` is expected: keep this file.

## License

GPLv3, as Aurora Viewer. See [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).
