# Boot animation

Two cold-start variants, one picked at random per launch by `crates/app/src/boot_splash/`
(full-window video over the first window; any key or click skips; reduce motion skips it):

- `boot-sync.mp4` — "Sync": monochrome hologram renders her, neural sync climbs to 100%, she opens her eyes and colour spreads from her irises.
- `boot-awaken.mp4` — "Awaken" (9.0s): her vision system boots — a scan renders her closed eyes (full kf-S01 frame), two reticles lock onto her eyes while initialization climbs to 100%, the HUD collapses into her irises and she opens her eyes; the mark rises from the dark. Footage is motion-interpolated to 60fps.

Sync is 6.0s, Awaken 9.0s; both 1920x1080, 60fps, silent. Source project: `../norma-intro-video`
(Remotion compositions `BootSync` and `BootAwaken`, design in `docs/boot-plan.md` there):

    npx remotion render BootSync out/boot-v2/boot-main-silent.mp4 --props='{"silent":true}' --muted
    npx remotion render BootAwaken out/boot-awaken/boot-awaken.mp4 --muted

If a length changes, update `variant_duration` in `boot_splash/mod.rs`.
