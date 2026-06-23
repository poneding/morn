# File Association Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add "Open with morn" right-click context menu for video files on macOS, Windows, and Linux.

**Architecture:** Three platform packaging scripts declare file-type associations (macOS Info.plist, Linux .desktop MimeType, Windows registry via cargo-packager). The app's `main.rs` reads CLI args and passes them into `PlayerApp` which issues `Command::OpenFiles(paths)` — the same path already used by drag-and-drop.

**Tech Stack:** Rust (main.rs), bash (package-macos.sh, package-linux.sh), PowerShell (package-windows.ps1), cargo-packager config

**Spec:** `docs/superpowers/specs/2026-06-22-file-association-design.md`

---

### Task 1: CLI argument handling in main.rs

**Files:**
- Modify: `crates/app/src/main.rs`
- Modify: `crates/app/src/app.rs`

**Context:** All three platforms pass file paths as CLI args when "Open with" is used. Currently `main()` ignores args. We need to collect them, filter to video files, and pass them into `PlayerApp` so it issues `Command::OpenFiles(paths)` at startup.

**Design:**
- `main()` reads `std::env::args_os()`, skips arg[0], resolves to absolute paths, filters to video extensions
- `PlayerApp::new()` gains `initial_paths: Vec<PathBuf>` parameter
- In `new()`, if `initial_paths` is non-empty, calls `self.player.handle(Command::OpenFiles(initial_paths))`

- [ ] **Step 1: Add `initial_paths` to `PlayerApp::new()` and store in a field**

In `app.rs`, add field `initial_paths: Vec<PathBuf>` to `PlayerApp`.
Modify constructor to accept and store it.
After the existing restore logic, if `initial_paths` is non-empty, call `self.player.handle(Command::OpenFiles(initial_paths))`.

- [ ] **Step 2: Add CLI arg parsing to `main.rs`**

In `main.rs`, before `eframe::run_native()`, read args, resolve to absolute paths, filter by video extension, pass into `PlayerApp`.

Define supported video extensions as a static list:
```rust
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v",
    "mpg", "mpeg", "ts", "mts", "m2ts", "3gp", "ogv", "rm", "rmvb",
];
```

Helper function `collect_cli_video_paths() -> Vec<PathBuf>`:
```rust
fn collect_cli_video_paths() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .filter_map(|arg| {
            let path = std::path::PathBuf::from(arg);
            if !path.is_file() { return None; }
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            VIDEO_EXTENSIONS.contains(&ext.as_str()).then(|| {
                path.canonicalize().unwrap_or(path)
            })
        })
        .collect()
}
```

- [ ] **Step 3: LSP diagnostics check**

Run: `lsp_diagnostics("crates/app/src/main.rs")` and `lsp_diagnostics("crates/app/src/app.rs")`
Expected: No errors.

- [ ] **Step 4: Run existing tests to verify no regressions**

Run: `cargo test -p app --lib`
Expected: All passing.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/main.rs crates/app/src/app.rs
git commit -m "feat: accept video files via CLI arguments for 'Open with' integration"
```

---

### Task 2: macOS — Info.plist document type declarations

**Files:**
- Modify: `scripts/release/package-macos.sh`

**Context:** macOS uses `CFBundleDocumentTypes` in Info.plist to declare which file types the app can open. The file manager reads these to populate the "Open With" menu.

- [ ] **Step 1: Add `CFBundleDocumentTypes` to Info.plist template in `package-macos.sh`**

After the existing `LSMinimumSystemVersion` / `LSApplicationCategoryType` entries, insert:

```xml
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>MPEG-4 Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.mpeg-4</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Matroska Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>org.matroska.mkv</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>QuickTime Movie</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>com.apple.quicktime-movie</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>AVI Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.avi</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Windows Media Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>com.microsoft.windows-media-wmv</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Flash Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>com.adobe.flash-video</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>WebM Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>org.webmproject.webm</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>MPEG Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.mpeg</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>MPEG-2 Transport Stream</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.mpeg-2-transport-stream</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>3GPP Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>public.3gpp</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Ogg Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>org.xiph.ogv-video</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>RealMedia Video</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array><string>com.real.realmedia</string></array>
    </dict>
  </array>
```

- [ ] **Step 2: Verify the edit looks correct**

Read back the Info.plist section to confirm valid XML.

- [ ] **Step 3: Commit**

```bash
git add scripts/release/package-macos.sh
git commit -m "feat(macos): add CFBundleDocumentTypes for 'Open with' context menu"
```

---

### Task 3: Linux — .desktop MimeType entries

**Files:**
- Modify: `scripts/release/package-linux.sh`

**Context:** The `.desktop` file is generated by `package-linux.sh`. Adding a `MimeType=` line tells the desktop environment which file types the app supports, enabling "Open with" in file managers (Nautilus, Dolphin, Thunar, etc.).

- [ ] **Step 1: Add `MimeType` to the .desktop template**

In `package-linux.sh`, the `cat > "${desktop}" <<'EOF'` block. Add line after `Categories=AudioVideo;Player;Video;`:

```
MimeType=video/mp4;video/x-matroska;video/quicktime;video/x-msvideo;video/x-ms-wmv;video/x-flv;video/webm;video/mpeg;video/mp2t;video/3gpp;video/ogg;video/vnd.rn-realvideo;video/x-m4v;
```

- [ ] **Step 2: Commit**

```bash
git add scripts/release/package-linux.sh
git commit -m "feat(linux): add MimeType entries to .desktop for 'Open with' support"
```

---

### Task 4: Windows — Installer registry entries

**Files:**
- Modify: `scripts/release/package-windows.ps1`

**Context:** `cargo-packager` supports `[[registry]]` config entries that write to the Windows Registry during installation. We register morn.exe as an "Open with" option for each video extension.

- [ ] **Step 1: Add `[[registry]]` entries to the cargo-packager TOML config**

In `package-windows.ps1`, after the `[[binaries]]` section in the TOML config string, add registry entries for each video extension.

The standard pattern for "Open with" registration in Windows:
1. Under `HKLM\Software\Classes\Applications\morn.exe\SupportedTypes`: list all supported extensions (set value to empty string)
2. Under `HKLM\Software\Classes\.ext\OpenWithList\morn.exe`: for each extension, add morn.exe to the OpenWithList

Using cargo-packager's `[[registry]]` syntax:

```toml
[[registry]]
hkcu = false
key = "Software\\Classes\\Applications\\morn.exe\\SupportedTypes"
value = ".mp4"
# (delete existing content and set default to empty)

[[registry]]
hkcu = false
key = "Software\\Classes\\.mp4\\OpenWithList\\morn.exe"
value = ""
```

Actually, cargo-packager's registry format per the docs supports creating keys and setting values. Let me use the approach of creating the SupportedTypes key with each extension, and adding OpenWithList entries.

Simpler approach: Use `HKLM\Software\Classes\Applications\morn.exe\SupportedTypes` with each extension as a named value (empty string), and add the app to the OpenWithProgIds.

The most practical approach for cargo-packager 0.11.x:

```toml
[[registry]]
hkcu = false
key = "Software\\Classes\\Applications\\morn.exe\\SupportedTypes"
name = ".mp4"
value = ""
```

But since we have many extensions, we should generate these entries programmatically in the PowerShell script.

In the PowerShell, after constructing the base config, append registry entries for each extension:

```powershell
$extensions = @(
    ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v",
    ".mpg", ".mpeg", ".ts", ".mts", ".m2ts", ".3gp", ".ogv", ".rm", ".rmvb"
)

$registryConfig = @"
[[registry]]
hkcu = false
key = "Software\\Classes\\Applications\\morn.exe\\SupportedTypes"
"@

foreach ($ext in $extensions) {
    $registryConfig += @"

[[registry]]
hkcu = false
key = "Software\\Classes\\Applications\\morn.exe\\SupportedTypes"
name = "$ext"
value = ""
"@
}

# Also set morn.exe as an OpenWithList entry for common extensions
foreach ($ext in $extensions) {
    $registryConfig += @"

[[registry]]
hkcu = false
key = "Software\\Classes\\$ext\\OpenWithList\\morn.exe"
value = ""
"@
}

$config += "`n" + $registryConfig
```

- [ ] **Step 2: Commit**

```bash
git add scripts/release/package-windows.ps1
git commit -m "feat(windows): add registry entries for 'Open with morn' context menu"
```

---

### Task 5: Verify — build check

- [ ] **Step 1: Verify Rust code compiles**

Run: `cargo check -p app`
Expected: Compilation succeeds with no errors.

- [ ] **Step 2: Run tests**

Run: `cargo test -p app --lib`
Expected: All tests pass.

- [ ] **Step 3: Final review of all changes**

Read back each modified file to confirm correctness.
