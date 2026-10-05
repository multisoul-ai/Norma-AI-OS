/// Material's adaptive grid keeps comfortable targets instead of squeezing four columns.
pub(super) struct WallpaperLayout {
    pub width: f32,
    pub columns: usize,
    pub cell_width: f32,
    pub stacked_preview: bool,
}

impl WallpaperLayout {
    pub const GAP: f32 = 12.0;

    pub fn for_width(available: f32) -> Self {
        let width = available.clamp(1.0, 856.0);
        let columns = (((width + Self::GAP) / (128.0 + Self::GAP)) as usize).clamp(1, 4);
        let cell_width = (width - Self::GAP * (columns - 1) as f32) / columns as f32;
        Self {
            width,
            columns,
            cell_width,
            stacked_preview: width < 620.0,
        }
    }
}
