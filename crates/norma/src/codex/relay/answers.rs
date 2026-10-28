//! Final send qualification lives on the same thread and cancellation lock as the native RPC.
use super::*;
use crate::agent_hook::{HookQuestion, NativeQuestion};
use std::io;
use std::time::Instant;

impl CodexQuestionRelay {
    pub fn answer_ticket(
        &self,
        question: CodexQuestionLease,
        answers: Vec<QuestionAnswer>,
        delivery_id: String,
        ticket: HookQuestion,
    ) -> Result<Receiver<AnswerResult>, String> {
        if !self.is_running() || !self.is_observing() {
            return Err("codex_question_relay_unavailable".into());
        }
        if question.owner != self.owner {
            return Err("codex_question_connection_changed".into());
        }
        let NativeQuestion::Codex {
            question: original,
            connection_id,
        } = ticket.adapter()
        else {
            return Err("codex_question_ticket_provider".into());
        };
        if original != &question.question
            || connection_id != &question.connection_id()
            || !ticket.matches_delivery(&delivery_id, &answers)
        {
            return Err("codex_question_delivery_changed".into());
        }
        question
            .question
            .response(&answers)
            .map_err(str::to_owned)?;
        let (result, receiver) = bounded(1);
        self.commands
            .try_send(Answer {
                question,
                answers,
                result,
                delivery: Some((delivery_id, ticket)),
            })
            .map_err(|_| "codex_question_queue_full")?;
        Ok(receiver)
    }
}

impl Relay {
    pub(super) fn send_answer(&mut self, command: Answer) -> Result<bool, String> {
        if command.question.generation != self.generation
            || !self.starts.is_empty()
            || !self.observing.load(Ordering::Acquire)
        {
            let _ = command
                .result
                .try_send(Err("codex_question_attachment_changed".into()));
            return Ok(true);
        }
        let ledger = &mut self.ledger;
        let server = &mut self.server;
        let prepare = || {
            let ledger = ledger.as_mut().ok_or("codex_question_session_missing")?;
            let delivery_id = command.delivery.as_ref().map(|(id, _)| id.as_str());
            ledger.reply_delivery(&command.question.question, &command.answers, delivery_id)
        };
        let write = |frame: Value| {
            send(server, Message::Text(frame.to_string().into())).map_err(io::Error::other)
        };
        let result = guarded_write(&command, prepare, write);
        match result {
            Ok(true) => {
                self.answer_sent(&command);
                Ok(true)
            }
            Ok(false) => {
                self.pending_send = Some(command);
                Ok(false)
            }
            Err(error) => {
                let _ = command.result.try_send(Err(error.to_string()));
                if error.kind() == io::ErrorKind::Other {
                    Err(error.to_string())
                } else {
                    Ok(true)
                }
            }
        }
    }

    pub(super) fn flush_answer(&mut self) -> Result<bool, String> {
        let Some(command) = self.pending_send.take() else {
            return flush(&mut self.server);
        };
        match guarded_write(
            &command,
            || Ok(()),
            |_| flush(&mut self.server).map_err(io::Error::other),
        ) {
            Ok(true) => {
                self.answer_sent(&command);
                Ok(true)
            }
            Ok(false) => {
                self.pending_send = Some(command);
                Ok(false)
            }
            Err(error) => {
                let _ = command.result.try_send(Err(error.to_string()));
                Err(error.to_string())
            }
        }
    }

    pub(super) fn close_answers(&mut self, reason: &str) {
        if let Some(command) = self.pending_send.take() {
            if let Some((delivery_id, ticket)) = &command.delivery {
                let _ = ticket.with_native_send(delivery_id, Instant::now(), || {
                    Err(io::Error::other(reason))
                });
            }
            let _ = command.result.try_send(Err(reason.into()));
        }
        while let Ok(command) = self.commands.try_recv() {
            if let Some((_, ticket)) = &command.delivery {
                ticket.cancel();
            }
            let _ = command.result.try_send(Err(reason.into()));
        }
    }

    fn answer_sent(&mut self, command: &Answer) {
        if let Some(ledger) = &mut self.ledger {
            ledger.sent(&command.question.question);
        }
        self.publish_changes();
        let _ = command.result.try_send(Ok(()));
    }
}

fn guarded_write<T>(
    command: &Answer,
    prepare: impl FnOnce() -> Result<T, &'static str>,
    write: impl FnOnce(T) -> io::Result<bool>,
) -> io::Result<bool> {
    if let Some((delivery_id, ticket)) = &command.delivery {
        ticket.with_native_send_prepared(delivery_id, Instant::now(), prepare, write)
    } else {
        write(prepare().map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?)
    }
}
