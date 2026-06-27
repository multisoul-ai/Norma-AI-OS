use crate::SpeechError;

pub(crate) struct AudioCapture;
impl AudioCapture {
    pub(crate) fn start(_sender: crossbeam_channel::Sender<Vec<f32>>) -> Result<Self, SpeechError> {
        unimplemented!("audio capture stub")
    }
    pub(crate) fn stop(self) {}
}
