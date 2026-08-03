use super::{execute_card_command_for_area_with_strategy, NormaExecution};
use crate::motion::MotionStore;
use canvas::CanvasRect;
use norma::protocol::{CardCommand, CardCommandError};
use runtime::Runtime;

pub(crate) fn execute_card_command_for_area(
    runtime: &mut Runtime,
    motion: &mut MotionStore,
    master_layout: &mut crate::canvas::master_layout::MasterLayoutState,
    command: CardCommand,
    area: CanvasRect,
    zoom: f32,
) -> Result<NormaExecution, CardCommandError> {
    execute_card_command_for_area_with_strategy(
        runtime,
        motion,
        master_layout,
        crate::canvas::layout_strategy::CanvasLayoutStrategy::default(),
        command,
        area,
        zoom,
    )
}

pub(crate) fn text_for_submit(mut text: String, submit: bool) -> String {
    if submit && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
