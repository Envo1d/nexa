# Nexa GPUI Prototype

Current UI verification policy (PLAN-356, 2026-11-02): the user authorizes the agent to launch and verify only the developed Nexa window and its verifiably owned dialogs. Use the Computer Use skill with executable/process/window identification and window-only captures. Other application windows and desktop-wide capture are excluded. This supersedes earlier blanket manual-only verification instructions below, but does not change historical results or mark pending checks as passed. See the parent and prototype AGENTS.md Testing Guidelines; production-data/file-mutation restrictions remain unchanged.

Latest verification (PLAN-343, 2026-11-02): the user reports stable operation after PLAN-342 and previously confirmed working video/previews. This supersedes older pending basic-playback notes below. It is not an exhaustive test report or a confirmation of hardware decode/performance; the remaining feasibility gates still apply.

Windows-only, WebView-free prototype of Nexa. The copied Vue/Tauri implementation in the parent directory is a reference, not a runtime dependency. Active development continues in `E:/Coding/Projects/Rust/nexa/prototype`, within the current `nexa` repository.

## Run

From the project root:

```powershell
cargo run --manifest-path prototype/Cargo.toml --locked
```

The executable starts with 10,000 demo records using the 12 copied WebP fixtures. It does not open NexaNew settings/database, monitor folders, move files, install updates or persist configuration. Logs go to the console; use `RUST_LOG` to override the default filter.

- Search filters demo titles or selected filenames using case-insensitive substrings. This is a prototype filename search, not Nexa's production exact metadata search.
- Select a card and use **Open selected** for its bounded preview.
- **Open files** uses a native picker and replaces the demo set with at most 64 selected files. Supported entries receive up-to-512-pixel Shell thumbnails in one background task. Files stay read-only; the Windows Shell may maintain its own system thumbnail cache.
- **All files**, **Videos** and **Demo library** change the current view. The grid virtualizes equal-height rows and adapts its column count to window width. Multi-selection, drag selection and retained scroll anchors are later work.

The new executable has no active Tauri/Wry/WebView dependency. `prototype/Cargo.lock` can contain optional/platform-only packages which are not in the active build graph.

PLAN-340: Startup selects GPUI's native-child-compatible HWND DirectX swapchain path. GPUI 0.2.2's default topmost DirectComposition UI would cover the embedded video. This is process-local, preserves GPU rendering and requires no Windows setting changes. Shell previews now run on a fresh dedicated thread per batch so GPUI's MTA worker apartments do not conflict with the provider's STA initialization. Actual video visibility still needs manual verification after this fix.

PLAN-341: The user confirmed video and previews work after PLAN-340. Final-window close now cancels polling, releases playback and requests graceful app quit before default native destruction; this addresses the pinned GPUI invalid-HWND shutdown ordering. Clean close after this change still needs manual confirmation. Debug builds may separately report that the optional DXGI debug interface is unavailable; GPUI continues without those debugging features.

PLAN-342: Open files and Media engine temporarily leave fullscreen before the native dialog and restore it after selection or cancellation. Requests are serialized; transition waits are bounded. This mitigates a user-reported fullscreen picker native exit (0xcfffffff), but the exact cause and success after this change remain unconfirmed. Manual checks should cover choosing/cancelling, repeating both dialogs from fullscreen and closing during transitions. New logs identify `open_media`/`select_media_engine` stages without recording paths.

## Native Video

A compatible **64-bit libmpv DLL** and its dependent native libraries must be supplied separately. No media engine is bundled/downloaded or loaded by searching the current directory. Select an existing absolute path using **Media engine**, or pass it explicitly:

```powershell
cargo run --manifest-path prototype/Cargo.toml --locked -- --mpv-dll "C:\MediaEngine\mpv-2.dll"
```

Use a trusted libmpv distribution matching the executable architecture. The upstream [mpv installation page](https://mpv.io/installation/) links distribution options; retain the selected build's provenance/license information before future bundling. The [client API](https://github.com/mpv-player/mpv/blob/master/include/mpv/client.h) documents playback lifecycle and embedding.

Choose a local video with **Open files**, select its card, then **Open selected**. Playback is requested through an owned child HWND using `vo=gpu`, `gpu-context=d3d11`, and `hwdec=auto-safe`. Normal libmpv configuration/scripts are disabled. The player does not upload decoded CPU frames through GPUI images. Commands are asynchronous and events are polled without blocking; load/command failures appear in the status line. Initial engine setup and final teardown still happen on the UI thread and require latency verification.

Controls: play/pause, seek +/-10 seconds, mute and GPUI window fullscreen remain outside the native picture as fallback. PLAN-344 also explicitly enables libmpv's built-in Lua OSC inside the rendered picture, initially always visible. **Hide video controls / Show video controls** requests never/always visibility for an overlay comparison without replacing the player. Requires a libmpv build with Lua/OSC support. Scripts from user directories remain disabled. Use the external **Fullscreen** button: mpv's own fullscreen widget does not own the GPUI window. This proves a candidate mpv-rendered control path only after manual verification, not arbitrary GPUI widgets above the child HWND. Production theme/focus integration is still open.

## Decoder and Performance Gate - PLAN-344

The status line and console `playback_sample` logs publish observed libmpv properties once per second. Property observations are asynchronous and consumed by the existing bounded nonblocking event poll; there are no synchronous per-frame queries or CPU frame copies. Decoder/frame metrics can be unavailable with a particular engine or output; `unknown` is not zero or success. Playback failures remain visible instead of being replaced by routine telemetry.

- `hwdec-current=no` means software decoding; `d3d11va` or another active API reports hardware decoding. A `-copy` backend is hardware decode with a copy-back path, not zero-copy composition. The requested `auto-safe` setting alone is not evidence.
- `container-fps` describes source cadence; `estimated-vf-fps` estimates filter output; `display-fps` is the reported display rate. None certifies physical presented FPS. `current-vo` reports actual video output, not measured GPU use.
- `frame-drop-count` and `decoder-frame-drop-count` distinguish output/decoder drops. Compare deltas over uninterrupted playback; seeks/reloads may reset counters. `avsync` is the reported audio/video timing difference in seconds. Unsupported properties remain unknown.

Repeatable user-owned measurement (no automated GUI launch):

1. Run the rebuilt executable with the same trusted DLL and a known local high-frame-rate file. Record engine version/provenance, codec/resolution/source FPS, GPU/driver and display refresh. Keep window size, power mode and other load constant.
2. Allow 10 seconds of warm-up. Observe/log 60 seconds without seeking for each: windowed controls hidden, windowed controls visible, fullscreen controls hidden, fullscreen controls visible. Use the GPUI fullscreen button. Repeat while moving the pointer over the picture and interacting with OSC pause/seek/volume; record those interactions separately from uninterrupted samples.
3. Capture the first/last counters, selected `hwdec-current`, timing range and visible stutter for each interval. On Windows Task Manager record this process's CPU and the appropriate GPU Video Decode/3D engines with the same sampling conditions. Telemetry does not itself measure CPU/GPU utilization or renderer latency.
4. Verify OSC is visibly above moving video, mouse controls respond, video bounds do not shrink, and playback survives resize/DPI/fullscreen transitions. The toggle is a request, not proof that the Lua script loaded. If absent, retain external controls and report the selected DLL build; do not declare overlay success.
5. Return console samples and observations. Hardware decode, performance and overlay gates remain pending until those runtime results exist. A smooth image or zero reported drops alone does not establish 120-Hz physical output.

Reference contracts: [libmpv client API](https://github.com/mpv-player/mpv/blob/master/include/mpv/client.h), [mpv properties/options](https://mpv.io/manual/stable/), [built-in OSC](https://github.com/mpv-player/mpv/blob/master/DOCS/man/osc.rst).


No GUI launch or video playback was performed by the agent. Native file dialog behavior, actual decoder/renderer, child-window composition, repeated fullscreen/resize/DPI/display changes, audio sync, focus and resource cleanup need the user's manual verification.

## Checks

```powershell
cargo fmt --manifest-path prototype/Cargo.toml --check
cargo clippy --manifest-path prototype/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path prototype/Cargo.toml --locked -- --test-threads=1
cargo build --manifest-path prototype/Cargo.toml --locked
```

GPUI is pinned to 0.2.2 and GPUI Component to 0.5.1; its optional WebView feature is disabled. These known local release APIs were selected for this first spike rather than the newer moving upstream API used in the feasibility research. Reevaluate a supported upgrade path before treating this version pair as the long-term platform.

`main.rs` owns startup, `catalog.rs` demo/filter contracts, `ui.rs` the initial gallery, `previews.rs` the fresh thumbnail worker and `media.rs` the native playback lifetime. Existing standalone `media_thumbnail.rs` and `windows_com.rs` are reused directly from the copied backend through explicit module paths; the Tauri backend crate is not linked. The existing thumbnail regressions are therefore exercised by prototype tests too.

This first implementation slice does not complete the 5-10 day feasibility gate. Next: execute the PLAN-344 manual protocol with a pinned libmpv build, collect decoder/frame observations and validate OSC composition/input and focus. Choose a render-API/shared-texture approach if arbitrary GPUI overlays are required. Full-resolution image viewing and production data/core extraction follow after that gate.


## Native Feedback and OSC Input Follow-up - PLAN-345 (2026-11-02)

The user confirms the in-picture OSC is visible and playback is smooth, but its controls did not respond. Their snapshot reports hwdec-current=d3d11va, current-vo=gpu, H.264/AVC, container-fps=47.976437, estimated-vf-fps=47.758099, display-fps=144, both drop counters=0 and avsync=0. This confirms reported hardware decoding for that sample and visible mpv overlay composition. It is not a timed benchmark, a CPU/GPU utilization measurement or 60+ unique video frames per second. Approximately 48-fps video on a 144-Hz screen can appear smooth (roughly three refreshes per video frame); the screenshot alone cannot establish actual scanout cadence.

PLAN-345 enables native VO input and default-level script bindings, while disabling loading of stock mpv input bindings separately. The earlier policy disabled native VO keyboard input and default-level bindings. Upstream OSC versions can use forced bindings, so that setting alone is not a proven cause of this particular mouse failure. This is a targeted input-policy correction pending native verification, not proof of repaired Windows event delivery. No custom hooks, input forwarding, render-path changes or frame copies were added. Test hover, play/pause click, timeline dragging, volume and Hide/Show in both modes after restarting the rebuilt EXE. Use the external GPUI Fullscreen button; report if hover reacts but clicks do not, or neither reacts. Stock mpv shortcuts remain unloaded; future keyboard integration remains host work.


## OSC Hit Testing Follow-up - PLAN-346 (2026-11-02)

The user confirms PLAN-345 did not restore interaction with the in-picture control strip. The owned STATIC HWND had no SS_NOTIFY style; Windows documents its hit-test result as HTTRANSPARENT in that case. PLAN-346 adds SS_NOTIFY to make that container mouse-targetable (HTCLIENT). This fixes a host hit-test defect; actual OSC mouse delivery still needs manual confirmation after restarting the rebuilt executable. Check pause click, timeline drag and volume in the circled mpv strip, separately from the external GPUI buttons. No GUI launch or interaction was performed by the agent.


## Explicit Pointer Route - PLAN-347 (2026-11-02)

The user confirms PLAN-346 also did not repair OSC input. PLAN-347 supersedes the native input route: the owned host and descendants remain visible but are disabled for native input, so GPUI owns mouse delivery. GPUI forwards positions in physical surface pixels and left-button down/up using libmpv commands. No video decode/presentation settings change. Leaving the video cancels a drag; foreground loss and unpressed motion also release held input. This slice forwards hover and left click/drag; mouse wheel/right-button actions are not forwarded yet. Use external Fullscreen, not OSC's window widget.

Restart the rebuilt EXE, open video and test hover over OSC, play/pause, seek dragging and clicking the speaker. Console events `video_pointer` report boundary crossed, `phase=down` and `phase=release`. Their presence confirms delivery to the command bridge, not successful execution of the OSC action; if still unresponsive, provide those lines and any native command error. No per-motion info logging or GUI automation was added. Native results remain pending.


## Working In-Picture Controls Confirmed - PLAN-348 (2026-11-02)

The user confirms interaction with in-picture controls works after PLAN-347's explicit GPUI-to-mpv pointer bridge. This supersedes prior pending basic interaction statements. Earlier user evidence also confirms smooth playback, visible overlay and reported d3d11va decoding for the tested setup. Exhaustive control/window/DPI coverage and timed performance measurements remain unverified; wheel/right-button forwarding, OSC-to-host fullscreen integration and arbitrary themed GPUI overlays are not implemented.


## Custom Nexa Panel - PLAN-349 (2026-11-02)

The standard mpv OSC is now replaced by an application-owned Lua/ASS panel: dark translucent background, violet accent timeline, play/pause, +/-10 seconds, elapsed/total time, mute, a draggable 0-100 volume bar and FULL for GPUI window fullscreen. Controls remain over the video without resizing it. Hide/Show targets this panel; external controls remain fallback. The panel is always visible until explicitly hidden (auto-hide is not implemented). Narrow-window typography, fullscreen, DPI and pointer interaction await manual verification.

The script is compiled into the EXE, explicitly loaded from an owned unique temporary file, and removed after playback teardown. No source checkout is required at runtime, no engine download and no external script-directory autoload. Requires the same Lua-capable engine used by the working stock OSC. Passive progress updates run at 4 Hz; drag/control feedback is immediate. The existing GPUI input bridge remains the only pointer route. FULL requires pressing and releasing on that button; it toggles the GPUI window rather than mpv's private fullscreen property.

After restarting: check panel appearance, pause/resume, both seek buttons, timeline dragging, duration-unavailable behavior, mute, volume dragging, Hide/Show, FULL and repeated viewer open/close. Actual Lua execution and visuals were not exercised by the agent; the Rust checks do not validate live Lua/mpv behavior. Script lifecycle/error logs are filtered without dumping media metadata. No standalone Lua runtime is installed for a separate syntax check.


### Missing Custom Panel Follow-up - PLAN-350

The user reported no panel after PLAN-349. ASS formatting strings contained invalid short-string Lua escapes; they now use long-bracket literals. Numeric overlay height/volume formatting is integer-safe, and log filtering accepts sanitized script names. Rebuilt and Rust checks passed; native Lua execution and appearance still require manual confirmation. Restart and reopen the video; external controls remain available if a panel error is reported.


### Seek Pressure Follow-up - PLAN-351

The user confirms the custom panel now appears. A generic error burst during seeking prompted bounds on raw input: changed mouse coordinates at most about 30 Hz, immediate real press/release positions, and 10-Hz keyframe seeking during timeline drag with an exact final seek. Consecutive duplicate failure reports are suppressed; command rejection/async errors now identify a safe command name and numeric code. These changes are rebuilt and Rust-tested; actual seeking still needs manual confirmation. If errors recur, return the new command/error-code line rather than the old generic message.

## Tauri Main Interface Port - PLAN-352 (2026-11-02)

The user requests that the new interface follow the existing Tauri application. The GPUI executable now implements the first Main slice from the current App.vue/styles/themes and the preserved PLAN-324 screenshot: custom 56-pixel titlebar and window controls, Main/Stage tabs, collapsed/expanded 60/146-pixel navigation, 220-pixel tag panel, collection search/type/layout controls, responsive virtualized cards and an explicitly toggled information drawer. Initial/minimum sizes match the reference configuration (1680x1000 / 960x600). This is an initial source-guided port; visual equivalence has not been verified.

- Search currently matches title, selected filename or tag by a case-insensitive substring. This does not implement production semicolon/exact metadata search or suggestions.
- Tag usage rows, Frequent/All, tagged-only/hide-untagged, preview visibility, session favorites, image/video filters and newest/oldest modification ordering work on isolated in-memory entries. Unknown real-file dates are displayed as unavailable; known filesystem modification times are used for sorting. Favorites are session-only.
- Single click selects without opening information. The information button explicitly toggles the drawer; double click or the card menu's Open action opens the existing preview/player. The first card menu contains Open and Information only. Grid and list both use bounded uniform-list rendering; filtering resets the scroll position.
- Demo titles, kinds, extensions, characters, tags, durations, favorites and fixture clock follow src/data/mockMedia.ts while retaining the prototype's 10,000-record stress dataset. Demo video cards are illustrations and open their WebP fixture, not native video playback.
- Open files / Media engine remain temporary prototype utility controls below the collection controls. Open files retains the existing bounded read-only picker and Shell preparation. Restart the executable to restore demo data after selecting real files.
- Stage, tag management, health, settings and duplicate-search entries explicitly report that their port is pending. Production catalog/settings, persistence, editor/distribution workflows, multi/drag selection, retained scroll anchors, complete details/actions, full-resolution image viewing, search shortcuts/suggestions, full sorting options, exact icon/theme/spacing parity and localization are subsequent work. No production data access was enabled.

UI responsibilities are split into ui/shell.rs, ui/filters.rs, ui/collection.rs and the embedded outline icon provider ui/assets.rs. Existing playback, native input bridge, fullscreen-dialog recovery and panel remain in their previous owners. No GUI/browser launch or screenshot capture was performed. The user permits browser reference inspection if necessary; existing local source/reference images were sufficient for this slice. Native appearance, window dragging/maximize/close, layout at narrow widths/DPI, menus, filters/details and playback after the shell change await manual verification. Earlier playback confirmations predate this shell change.

### Startup Hover Assertion Follow-up - PLAN-353

The user reported a first-frame "hover style already set" panic after PLAN-352. The titlebar close button registered hover twice through shared constructors. Hover color is now passed into the single base registration; the close control keeps its red hover. Formatting, strict Clippy, all 17 serial tests and the linked build pass. Restart the rebuilt executable to verify native startup; Rust checks alone do not render the first frame. The earlier DXGI debug-interface diagnostic and Cargo advisories are separate from the identified hover assertion.

### Official Component Icons - PLAN-354

The supplied screenshot confirms the shell starts after PLAN-353, but raw SVG slots were blank. UI icons now use gpui-component::Icon, which explicitly applies inherited tint, and the official gpui-component-assets 0.5.1 bundle. Nine variants absent from its default set use original Lucide 1.52.0 SVGs matching the reference frontend, documented in assets/icons/README.md. The manually drawn path registry is removed; metadata person/tag glyphs are also restored.

Formatting, strict Clippy, 19 serial tests and linked build pass. Offline regressions resolve/rasterize all 95 registered SVGs with nonempty alpha. Restart the executable to verify actual icon visibility, active/muted/favorite/metadata colors and hover at your DPI; no GUI or new screenshots were produced by the agent. This change does not complete density/typography/layout parity against Tauri.

### Main Geometry and Information Drawer - PLAN-355

The user's grid/list/details captures are compared with the authoritative Tauri Vue/CSS in ../docs/gpui-ui-comparison.md. Current Main uses the actual collection width, the source minimum card/gap formula, full-width 88px-pitch list rows, compact grid dates, type-count badges, a four-option sort menu, grouped layout controls and one raised search field with Ctrl+F/clear. Information is a 360px overlay with a close button/Escape, full filename, preview, attribution groups, known technical fields and read-only open/reveal/copy/favorite actions. Unknown real-file media properties remain unavailable.

The ordinary debug footer and extra Main utility row are removed. To select local files or libmpv from the library, use the settings-icon menu's Open files / Media engine items; the viewer toolbar also retains them. The settings page itself remains unported. Operation failures/fallback notices appear as dismissible messages; viewer playback diagnostics remain visible. Search now ANDs exact semicolon-separated metadata names with shared Unicode normalization; selected read-only files additionally support filename substring terms. Production suggestion popups are still unported.

Passed formatting, strict Clippy, 25 serial tests and linked build. No GUI/browser launch or new capture was performed. Restart and manually compare at matching size/DPI, including narrow/expanded navigation, list width, later grid rows, scrollbar, drawer open/close without reflow, keyboard focus, sort/count/filter interactions and playback/picker recovery. See the comparison report for remaining multi-selection, editor/workflow, localization and visual polish gaps. Existing confirmations of icon visibility/startup apply only to the user's pre-correction captures.
