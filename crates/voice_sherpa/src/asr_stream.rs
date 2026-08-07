use asr::{
    InputEndReason, RecognitionCancellation, RecognitionEvent, RecognitionPipeline,
    RecognitionSession, RecognizerFactory,
};
use audio::PcmAudioChunk;
use dialogue::{AsrClient, AsrEvent, AsrInputEndReason, DialogueError};

#[cfg(test)]
#[path = "../tests/unit/asr_stream_tests.rs"]
mod asr_stream_tests;

pub(crate) struct PipelineAsrStreamClient {
    factory: Box<dyn RecognizerFactory>,
    session: RecognitionSession,
    cancellation: RecognitionCancellation,
    terminal_consumed: bool,
}

impl PipelineAsrStreamClient {
    pub(crate) fn new(factory: Box<dyn RecognizerFactory>) -> Result<Self, DialogueError> {
        let cancellation = RecognitionCancellation::new();
        let session = start_session(factory.as_ref(), cancellation.child())?;
        Ok(Self {
            factory,
            session,
            cancellation,
            terminal_consumed: false,
        })
    }

    pub(crate) fn cancellation(&self) -> RecognitionCancellation {
        self.cancellation.clone()
    }

    fn collect_available(
        &mut self,
        block_until_terminal: bool,
    ) -> Result<Vec<AsrEvent>, DialogueError> {
        let events = if block_until_terminal {
            self.session.collect_events()
        } else {
            let receiver = self.session.events();
            let mut events = Vec::new();
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
            events
        };
        translate_events(events, &mut self.terminal_consumed)
    }

    fn replace_session(&mut self) -> Result<(), DialogueError> {
        let _ = self.session.cancel();
        self.session = start_session(self.factory.as_ref(), self.cancellation.child())?;
        self.terminal_consumed = false;
        Ok(())
    }
}

impl AsrClient for PipelineAsrStreamClient {
    fn push_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<AsrEvent>, DialogueError> {
        if self.terminal_consumed {
            return Ok(Vec::new());
        }
        match self.session.push_pcm(chunk.clone()) {
            Ok(()) => {}
            Err(asr::AsrError::SessionClosed) if self.session.reached_duration_limit() => {}
            Err(error) => return Err(DialogueError::Message(error.to_string())),
        }
        self.collect_available(self.session.reached_duration_limit())
    }

    fn end_utterance(&mut self) -> Result<Option<String>, DialogueError> {
        if self.terminal_consumed {
            return Ok(None);
        }
        if let Err(error) = self.session.finish(InputEndReason::Vad) {
            if error != asr::AsrError::SessionClosed {
                return Err(DialogueError::Message(error.to_string()));
            }
        }
        let events = self.collect_available(true)?;
        Ok(events.into_iter().find_map(|event| match event {
            AsrEvent::Final(text) => Some(text),
            AsrEvent::Partial(_) | AsrEvent::InputEnded(_) => None,
        }))
    }

    fn reset(&mut self) {
        if let Err(error) = self.replace_session() {
            tracing::warn!(target: "norma_voice", %error, "failed to reset ASR pipeline");
        }
    }
}

fn start_session(
    factory: &dyn RecognizerFactory,
    cancellation: RecognitionCancellation,
) -> Result<RecognitionSession, DialogueError> {
    let recognizer = factory
        .create()
        .map_err(|error| DialogueError::Message(error.to_string()))?;
    Ok(RecognitionPipeline::new(factory.capabilities())
        .start_with_cancellation(recognizer, cancellation))
}

fn translate_events(
    events: Vec<RecognitionEvent>,
    terminal_consumed: &mut bool,
) -> Result<Vec<AsrEvent>, DialogueError> {
    let mut translated = Vec::new();
    for event in events {
        match event {
            RecognitionEvent::Partial(text) => translated.push(AsrEvent::Partial(text)),
            RecognitionEvent::InputEnded(reason) => {
                let reason = match reason {
                    InputEndReason::User => AsrInputEndReason::User,
                    InputEndReason::Vad => AsrInputEndReason::Vad,
                    InputEndReason::DurationLimit => AsrInputEndReason::DurationLimit,
                };
                translated.push(AsrEvent::InputEnded(reason));
            }
            RecognitionEvent::Final(text) => {
                *terminal_consumed = true;
                translated.push(AsrEvent::Final(text));
            }
            RecognitionEvent::Empty => *terminal_consumed = true,
            RecognitionEvent::Failed(error) => {
                *terminal_consumed = true;
                return Err(DialogueError::Message(error.to_string()));
            }
            RecognitionEvent::Cancelled => {
                *terminal_consumed = true;
                return Err(DialogueError::Message(
                    "recognition was cancelled".to_string(),
                ));
            }
        }
    }
    Ok(translated)
}
