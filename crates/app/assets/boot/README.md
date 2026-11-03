# Boot animation

Cold launch plays `boot-trace.mp4` (`crates/app/src/boot_splash/`; full-window video over the first
window; any key or click skips; reduce motion skips it):

- `boot-trace.mp4` — "Trace" (10.767s, 646 frames): glowing lines trace Norma, weave into a mesh,
  materialize, she opens her eyes with data in her irises while a staged boot bar resolves into the
  Norma OS wordmark. Its last frame is the first frame of the default wallpaper
  `wallpapers/norma-awake.mp4` (an 8s seamless loop, 24fps), so the fade-out reveals the same
  scene. The wallpaper decoder starts under the splash, so the reveal lands mid-loop, not on frame 0.

Older variants stay bundled for acceptance only (`NORMA_E2E_BOOT_ANIMATION=sync|awaken`):

- `boot-sync.mp4` — "Sync" (6.0s): monochrome hologram renders her, neural sync climbs to 100%.
- `boot-awaken.mp4` — "Awaken" (9.0s): a scan renders her closed eyes, reticles lock on, she opens them.

Videos are 1920x1080, 60fps, silent. `boot-trace.m4a` is Trace's soundtrack (BGM + sfx, AAC 48kHz
stereo), played by `boot_splash/sound.rs` from the first video frame; the older variants have none.
Source project: `../norma-intro-video` (Remotion):

    npx remotion render BootTrace3 out/boot-trace3/boot-trace-silent.mp4 --muted
    npx remotion render BootWallpaper out/wallpaper/norma-wallpaper-loop.mp4 --props='{"clip":"loop-d"}' --muted
    npx remotion render BootTrace3 out/boot-trace3/boot-trace3-realistic.mp4 --props='{"bgm":"a"}'
    ffmpeg -i out/boot-trace3/boot-trace3-realistic.mp4 -vn -c:a copy -movflags +faststart boot-trace.m4a
    npx remotion render BootSync out/boot-v2/boot-main-silent.mp4 --props='{"silent":true}' --muted
    npx remotion render BootAwaken out/boot-awaken/boot-awaken.mp4 --muted

If a length changes, update `variant_duration` in `boot_splash/mod.rs`. If the wallpaper changes,
update its bytes/SHA-256 in `packaging/media/resources.json`.
