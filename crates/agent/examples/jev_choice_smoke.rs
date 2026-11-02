//! Explicit synthetic-only probe; never reads user audio, transcripts, storage or a roster.
use agent::jev_choice::{JevChoiceClient, JevChoiceRequest};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("TYPESAFE_API_KEY")?;
    let client = JevChoiceClient::new(key)?;
    let request = JevChoiceRequest::new(
        "Henry，帮我检查 Bruno 写的方案。".into(),
        BTreeMap::from([
            (
                "PLAN_0".into(),
                "Henry executes a review; Bruno is only the author mentioned.".into(),
            ),
            ("PLAN_1".into(), "Bruno executes the review.".into()),
            (
                "CLARIFY".into(),
                "The action or executor is ambiguous; ask only for clarification.".into(),
            ),
        ]),
    )?;
    let started = Instant::now();
    let answer = client
        .observe(&request, Duration::from_secs(15), CancellationToken::new())
        .await?;
    let matched = answer.choice == "PLAN_0";
    let result = serde_json::json!({"synthetic_only":true,"production_enabled":false,
        "requested_model":"jev-latest","observation":answer,
        "elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"expected_choice_matched":matched});
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, serde_json::to_vec_pretty(&result)?)?;
    }
    println!("{}", serde_json::to_string(&result)?);
    if !matched {
        return Err("synthetic plan choice did not match".into());
    }
    Ok(())
}
