# "Open with morn" — System File Association Design

## Overview

Add right-click "Open with morn" integration for video files across all three
target platforms (macOS, Windows, Linux).  When a user right-clicks a video file
in the OS file manager, "Open with Morn" should appear as an option; selecting
it launches Morn (or brings it to front) and opens the file(s).

## Video Formats to Register

Common desktop video formats:
`.mp4`, `.mkv`, `.avi`, `.mov`, `.wmv`, `.flv`, `.webm`, `.m4v`, `.mpg`,
`.mpeg`, `.ts`, `.mts`, `.m2ts`, `.3gp`, `.ogv`, `.rm`, `.rmvb`

## Behavioral Design

- Multiple files selected → all files are appended to the playlist; the first
  one starts playing immediately.
- If Morn is already running → the current behavior applies (each launch is a
  separate process, since Morn is a single-instance desktop app).
- The app already supports `Command::OpenFiles(Vec<PathBuf>)` which handles
  append-to-playlist + play-first.

## Implementation Layers

### Layer 1: CLI argument handling (crates/app/src/main.rs)

**Problem:** `main()` does not read `std::env::args()`.  All three platforms
pass file paths to the app as command-line arguments when invoked via
"Open with".

**Solution:** Before `eframe::run_native()`, collect `std::env::args_os()`,
skip the program name (arg 0), filter to paths that exist on disk and match
known video extensions, resolve them to absolute paths, and pass them into
`PlayerApp`.  The app's constructor issues `Command::OpenFiles(paths)` if any
paths were provided.

**Key details:**
- `PlayerApp::new()` already accepts `cc: &eframe::CreationContext` — we add
  an `initial_paths: Vec<PathBuf>` parameter.
- Filtering by extension is optional (defense-in-depth): the engine already
  handles non-video files gracefully, but skipping them at the CLI layer avoids
  confusing the user.
- Subtitle files passed as CLI args are ignored (no current context to attach
  them to).

### Layer 2: macOS — Info.plist document types

**File:** `scripts/release/package-macos.sh`

**Change:** Add `CFBundleDocumentTypes` array to `Info.plist` declaring each
video UTI.  macOS maps file extensions to UTIs; the system uses these
declarations to populate the "Open with" menu.

UTIs to register: `public.mpeg-4` (.mp4, .m4v), `org.matroska.mkv`,
`com.apple.quicktime-movie` (.mov, .qt), `public.avi`, `com.microsoft.windows-media-wmv`,
`public.mpeg` (.mpg, .mpeg), `public.3gpp` (.3gp), `org.xiph.ogv-video`,
`com.real.realmedia` (.rm, .rmvb), plus dynamic UTIs for less common types.

The key `LSHandlerRank` is set to `Alternate` (not `Default` or `Owner`) so
Morn appears in the "Open With" submenu without overriding the system default
player.

### Layer 3: Linux — .desktop MimeType entries

**File:** `scripts/release/package-linux.sh`

**Change:** Add `MimeType=` line to the `.desktop` file template, listing all
video MIME types.  The desktop environment reads this for file association
registration.

Video MIME types: `video/mp4`, `video/x-matroska`, `video/quicktime`,
`video/x-msvideo`, `video/x-ms-wmv`, `video/mpeg`, `video/webm`,
`video/3gpp`, `video/ogg`, `video/vnd.rn-realvideo`, `video/mp2t`,
`video/x-m4v`, `video/x-flv`.

Add `MimeType` to fpm's `--category` (already `AudioVideo`) — fpm uses the
.desktop file content automatically for the generated packages.

### Layer 4: Windows — Installer registry entries

**File:** `scripts/release/package-windows.ps1`

**Change:** Add `[[registry]]` entries to the `cargo-packager` TOML config.
Each video extension gets a registration that associates it with `morn.exe`.

Schema:
- `HKLM\Software\Classes\Applications\morn.exe\SupportedTypes` — list all
  supported extensions (`.mp4`, `.mkv`, etc.)
- `HKLM\Software\Classes\.ext\OpenWithList\morn.exe` — for each extension,
  add morn.exe to the "Open with" list

cargo-packager 0.11.x supports `[[registry]]` in its config format.

## Files to Modify

| File | Change type |
|------|-------------|
| `crates/app/src/main.rs` | Add CLI arg handling |
| `crates/app/src/app.rs` | Accept `initial_paths` in constructor |
| `scripts/release/package-macos.sh` | Add CFBundleDocumentTypes to Info.plist |
| `scripts/release/package-linux.sh` | Add MimeType to .desktop file |
| `scripts/release/package-windows.ps1` | Add registry entries to packager config |

## Out of Scope

- "Open with" from a web browser (not a video player use case)
- System-wide default player registration (Morn stays as an alternate, not
  the default)
- URL scheme registration (morn://)
- Single-instance enforcement (separate process each launch — fine for a
  lightweight player)
