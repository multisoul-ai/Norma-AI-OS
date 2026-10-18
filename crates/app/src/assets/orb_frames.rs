//! Embedded idle and active frames for the desktop Norma orb.

const NORMA_ORB_FRAMES: [&[u8]; 32] = [
    include_bytes!("../../assets/icons/norma-orb-00.png"),
    include_bytes!("../../assets/icons/norma-orb-01.png"),
    include_bytes!("../../assets/icons/norma-orb-02.png"),
    include_bytes!("../../assets/icons/norma-orb-03.png"),
    include_bytes!("../../assets/icons/norma-orb-04.png"),
    include_bytes!("../../assets/icons/norma-orb-05.png"),
    include_bytes!("../../assets/icons/norma-orb-06.png"),
    include_bytes!("../../assets/icons/norma-orb-07.png"),
    include_bytes!("../../assets/icons/norma-orb-08.png"),
    include_bytes!("../../assets/icons/norma-orb-09.png"),
    include_bytes!("../../assets/icons/norma-orb-10.png"),
    include_bytes!("../../assets/icons/norma-orb-11.png"),
    include_bytes!("../../assets/icons/norma-orb-12.png"),
    include_bytes!("../../assets/icons/norma-orb-13.png"),
    include_bytes!("../../assets/icons/norma-orb-14.png"),
    include_bytes!("../../assets/icons/norma-orb-15.png"),
    include_bytes!("../../assets/icons/norma-orb-16.png"),
    include_bytes!("../../assets/icons/norma-orb-17.png"),
    include_bytes!("../../assets/icons/norma-orb-18.png"),
    include_bytes!("../../assets/icons/norma-orb-19.png"),
    include_bytes!("../../assets/icons/norma-orb-20.png"),
    include_bytes!("../../assets/icons/norma-orb-21.png"),
    include_bytes!("../../assets/icons/norma-orb-22.png"),
    include_bytes!("../../assets/icons/norma-orb-23.png"),
    include_bytes!("../../assets/icons/norma-orb-24.png"),
    include_bytes!("../../assets/icons/norma-orb-25.png"),
    include_bytes!("../../assets/icons/norma-orb-26.png"),
    include_bytes!("../../assets/icons/norma-orb-27.png"),
    include_bytes!("../../assets/icons/norma-orb-28.png"),
    include_bytes!("../../assets/icons/norma-orb-29.png"),
    include_bytes!("../../assets/icons/norma-orb-30.png"),
    include_bytes!("../../assets/icons/norma-orb-31.png"),
];
const NORMA_ORB_ACTIVE_FRAMES: [&[u8]; 32] = [
    include_bytes!("../../assets/icons/norma-orb-active-00.png"),
    include_bytes!("../../assets/icons/norma-orb-active-01.png"),
    include_bytes!("../../assets/icons/norma-orb-active-02.png"),
    include_bytes!("../../assets/icons/norma-orb-active-03.png"),
    include_bytes!("../../assets/icons/norma-orb-active-04.png"),
    include_bytes!("../../assets/icons/norma-orb-active-05.png"),
    include_bytes!("../../assets/icons/norma-orb-active-06.png"),
    include_bytes!("../../assets/icons/norma-orb-active-07.png"),
    include_bytes!("../../assets/icons/norma-orb-active-08.png"),
    include_bytes!("../../assets/icons/norma-orb-active-09.png"),
    include_bytes!("../../assets/icons/norma-orb-active-10.png"),
    include_bytes!("../../assets/icons/norma-orb-active-11.png"),
    include_bytes!("../../assets/icons/norma-orb-active-12.png"),
    include_bytes!("../../assets/icons/norma-orb-active-13.png"),
    include_bytes!("../../assets/icons/norma-orb-active-14.png"),
    include_bytes!("../../assets/icons/norma-orb-active-15.png"),
    include_bytes!("../../assets/icons/norma-orb-active-16.png"),
    include_bytes!("../../assets/icons/norma-orb-active-17.png"),
    include_bytes!("../../assets/icons/norma-orb-active-18.png"),
    include_bytes!("../../assets/icons/norma-orb-active-19.png"),
    include_bytes!("../../assets/icons/norma-orb-active-20.png"),
    include_bytes!("../../assets/icons/norma-orb-active-21.png"),
    include_bytes!("../../assets/icons/norma-orb-active-22.png"),
    include_bytes!("../../assets/icons/norma-orb-active-23.png"),
    include_bytes!("../../assets/icons/norma-orb-active-24.png"),
    include_bytes!("../../assets/icons/norma-orb-active-25.png"),
    include_bytes!("../../assets/icons/norma-orb-active-26.png"),
    include_bytes!("../../assets/icons/norma-orb-active-27.png"),
    include_bytes!("../../assets/icons/norma-orb-active-28.png"),
    include_bytes!("../../assets/icons/norma-orb-active-29.png"),
    include_bytes!("../../assets/icons/norma-orb-active-30.png"),
    include_bytes!("../../assets/icons/norma-orb-active-31.png"),
];

pub(super) fn frame(path: &str) -> Option<&'static [u8]> {
    let (suffix, frames) = if let Some(suffix) = path.strip_prefix("icons/norma-orb-active-") {
        (suffix, &NORMA_ORB_ACTIVE_FRAMES)
    } else {
        (path.strip_prefix("icons/norma-orb-")?, &NORMA_ORB_FRAMES)
    };
    let index = suffix.strip_suffix(".png")?.parse::<usize>().ok()?;
    frames.get(index).copied()
}

pub(super) const FRAME_COUNT: usize = NORMA_ORB_FRAMES.len();
