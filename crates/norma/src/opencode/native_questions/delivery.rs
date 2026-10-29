//! A single HTTP attempt, with foreground revalidation after the preparatory GET.
use super::*;

pub(super) fn deliver(
    client: &OpenCodeQuestionClient,
    waiting: &mut Waiting,
    running: &AtomicBool,
    foreground: &mut Option<Foreground>,
) -> Option<OpenCodeNativeEvent> {
    let raw = waiting.ticket.take_response(Instant::now())?;
    let reply = serde_json::from_value::<NativeQuestionReply>(raw).ok()?;
    let prepared = client.prepare_reply(&waiting.lease, &reply.answers);
    waiting.reply = Some(reply.clone());
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(reason) => {
            waiting.ticket.cancel();
            return Some(waiting.change(
                QuestionState::Failed,
                None,
                Some(format!("opencode_prepare_{reason:?}")),
            ));
        }
    };
    if !running.load(Ordering::Acquire) || foreground.as_mut().is_some_and(|f| !f.still_current()) {
        waiting.ticket.cancel();
        return None;
    }
    let sending = match waiting
        .ticket
        .begin_http_delivery(&reply.delivery_id, Instant::now())
    {
        Ok(sending) => sending,
        Err(reason) => {
            return Some(waiting.change(QuestionState::Failed, None, Some(reason.into())))
        }
    };
    match client.send_prepared(prepared) {
        Ok(()) => {
            sending.accepted();
            waiting.accepted = true;
            Some(waiting.change(QuestionState::Submitted, None, None))
        }
        Err(ReplyError::NotPending | ReplyError::Rejected) => {
            sending.rejected();
            Some(waiting.change(
                QuestionState::Failed,
                None,
                Some("opencode_reply_rejected".into()),
            ))
        }
        Err(_) => {
            drop(sending);
            waiting.uncertain = true;
            Some(waiting.change(
                QuestionState::Unknown,
                None,
                Some("question_transport_closed".into()),
            ))
        }
    }
}
