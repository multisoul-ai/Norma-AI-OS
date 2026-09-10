use super::*;
use agent::{PlanTodo, PlanTodoChoice};
use serde_json::json;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TodoDecisionRef {
    pub device_id: String,
    pub operation_id: String,
}

const ATTEMPTS: &str = "SELECT plan_id,todo_index,device_id,operation_id,choice_json,
    NULL AS previous_device_id,NULL AS previous_operation_id FROM project_todo_decisions
    UNION ALL SELECT plan_id,todo_index,device_id,operation_id,choice_json,
    previous_device_id,previous_operation_id FROM project_todo_retries";
const CURRENT: &str = "NOT EXISTS(SELECT 1 FROM project_todo_retries r
    WHERE r.previous_device_id=d.device_id AND r.previous_operation_id=d.operation_id)";

pub enum PlanTodoReservation {
    New(PlanTodo),
    Existing(MobileCommandReceipt),
    Conflict,
    AlreadyDecided,
}

#[derive(Debug)]
pub struct StoredTodoDecision {
    pub todo_index: usize,
    pub choice: PlanTodoChoice,
    pub receipt: MobileCommandReceipt,
    pub queued: Option<QueuedMessage>,
    pub retry_of: Option<TodoDecisionRef>,
    pub can_retry: bool,
}

impl MobileCommandStore {
    /// The single Todo decision and its existing command/queue entry commit together.
    pub fn reserve_plan_todo(
        &self,
        request: &NewMobileCommand,
        plan_id: &str,
        todo_index: usize,
        choice: &PlanTodoChoice,
        grant_revision: u64,
    ) -> StorageResult<PlanTodoReservation> {
        self.reserve_todo_attempt(request, plan_id, todo_index, choice, grant_revision, None)
    }

    /// Explicit new confirmation, conditional on the exact previous decision being proven unsent.
    pub fn retry_plan_todo(
        &self,
        request: &NewMobileCommand,
        plan_id: &str,
        todo_index: usize,
        choice: &PlanTodoChoice,
        grant_revision: u64,
        previous: &TodoDecisionRef,
    ) -> StorageResult<PlanTodoReservation> {
        self.reserve_todo_attempt(
            request,
            plan_id,
            todo_index,
            choice,
            grant_revision,
            Some(previous),
        )
    }

    fn reserve_todo_attempt(
        &self,
        request: &NewMobileCommand,
        plan_id: &str,
        todo_index: usize,
        choice: &PlanTodoChoice,
        grant_revision: u64,
        previous: Option<&TodoDecisionRef>,
    ) -> StorageResult<PlanTodoReservation> {
        validate(request)?;
        if let Some(previous) = previous {
            crate::mobile_access_store::validate_id(&previous.device_id)?;
            crate::mobile_access_store::validate_id(&previous.operation_id)?;
            if !matches!(choice, PlanTodoChoice::Accept { .. }) {
                return Err(invalid("retry must explicitly accept"));
            }
        }
        crate::mobile_access_store::validate_id(plan_id)?;
        if todo_index >= 8 || grant_revision == 0 {
            return Err(invalid("invalid todo request"));
        }
        let mut connection = open_app_connection(&self.path)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let plan = crate::project_plan_store::read(&tx, plan_id)?
            .filter(|plan| plan.workspace_id == request.workspace_id)
            .ok_or_else(|| invalid("plan not found"))?;
        let mut todo = plan
            .draft
            .todos
            .get(todo_index)
            .cloned()
            .ok_or_else(|| invalid("todo not found"))?;
        todo.validate_choice(choice).map_err(invalid)?;
        if let Some((mut receipt, owner, hash)) =
            read(&tx, &request.device_id, &request.operation_id)?
        {
            let source = attempt(&tx, &request.device_id, &request.operation_id)?;
            if hash != request.request_hash
                || receipt.workspace_id != request.workspace_id
                || receipt.route_id != request.route_id
                || !source.is_some_and(|(id, index, json, retry)| {
                    id == plan_id
                        && index == todo_index
                        && retry.as_ref() == previous
                        && serde_json::from_str::<PlanTodoChoice>(&json).ok().as_ref()
                            == Some(choice)
                })
            {
                return Ok(PlanTodoReservation::Conflict);
            }
            recover(&mut receipt, &owner, &request.owner_id);
            return Ok(PlanTodoReservation::Existing(receipt));
        }
        let current: Option<(String,String,String)> = tx.query_row(
            &format!("SELECT d.device_id,d.operation_id,d.choice_json FROM ({ATTEMPTS}) d WHERE d.plan_id=?1 AND d.todo_index=?2 AND {CURRENT}"),
            params![plan_id,todo_index], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        match (current, previous) {
            (None, None) => {}
            (Some((device, operation, json)), Some(previous))
                if device == previous.device_id && operation == previous.operation_id =>
            {
                let old_choice = serde_json::from_str::<PlanTodoChoice>(&json)?;
                let (receipt, _, _) = read(&tx, &device, &operation)?
                    .ok_or_else(|| invalid("missing previous receipt"))?;
                let queued = queue::read_queue(&tx, &device, &operation)?;
                if !retryable(&old_choice, &receipt, queued.as_ref(), &request.owner_id) {
                    return Ok(PlanTodoReservation::AlreadyDecided);
                }
                // An old owner's queued record is only a projection until this CAS. Freeze it
                // in the same transaction as the replacement so an old claimant cannot race us.
                if queued.as_ref().is_some_and(|q| q.state == "queued") {
                    tx.execute("UPDATE mobile_message_queue SET state='cancelled',error='owner_closed' WHERE device_id=?1 AND operation_id=?2 AND state='queued'",params![device,operation])?;
                }
            }
            _ => return Ok(PlanTodoReservation::AlreadyDecided),
        }
        let reserved = match choice {
            PlanTodoChoice::Accept {
                target,
                queue: true,
            } => {
                todo.target = target.clone();
                queue::enqueue_on(
                    &tx,
                    request,
                    &NewQueuedMessage {
                        node_id: target.node_id,
                        profile: target.profile.clone(),
                        run_id: target
                            .run_id
                            .clone()
                            .ok_or_else(|| invalid("queue requires run"))?,
                        session_id: target
                            .session_id
                            .clone()
                            .ok_or_else(|| invalid("queue requires session"))?,
                        grant_revision,
                        text: todo.instruction.clone(),
                    },
                )?
            }
            PlanTodoChoice::Accept {
                target,
                queue: false,
            } => {
                todo.target = target.clone();
                reserve_on(&tx, request)?
            }
            PlanTodoChoice::Reject => {
                let reserved = reserve_on(&tx, request)?;
                let result = json!({"plan_id":plan_id,"todo_index":todo_index,"status":"declined"})
                    .to_string();
                tx.execute("UPDATE mobile_commands SET state='succeeded',result_json=?1 WHERE device_id=?2 AND operation_id=?3",params![result,request.device_id,request.operation_id])?;
                reserved
            }
        };
        if !matches!(reserved, MobileCommandReservation::New) {
            return Err(invalid("todo command collision"));
        }
        if let Some(previous) = previous {
            tx.execute("INSERT INTO project_todo_retries(plan_id,todo_index,device_id,operation_id,choice_json,previous_device_id,previous_operation_id,created_at_unix) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![plan_id,todo_index,request.device_id,request.operation_id,serde_json::to_string(choice)?,previous.device_id,previous.operation_id,current_unix_timestamp()?])?;
        } else {
            tx.execute("INSERT INTO project_todo_decisions(plan_id,todo_index,device_id,operation_id,choice_json,created_at_unix) VALUES (?1,?2,?3,?4,?5,?6)",
                params![plan_id,todo_index,request.device_id,request.operation_id,serde_json::to_string(choice)?,current_unix_timestamp()?])?;
        }
        tx.commit()?;
        Ok(PlanTodoReservation::New(todo))
    }

    /// Shared project decisions may reference another authorized phone; credentials are never stored here.
    pub fn plan_todo_decisions(
        &self,
        workspace: &str,
        plan_id: &str,
        owner: &str,
    ) -> StorageResult<Vec<StoredTodoDecision>> {
        for id in [workspace, plan_id, owner] {
            crate::mobile_access_store::validate_id(id)?;
        }
        let connection = open_app_connection(&self.path)?;
        let mut query=connection.prepare(&format!("SELECT d.todo_index,d.choice_json,d.device_id,d.operation_id,d.previous_device_id,d.previous_operation_id FROM ({ATTEMPTS}) d JOIN project_plans p USING(plan_id) WHERE p.workspace_id=?1 AND d.plan_id=?2 AND {CURRENT} ORDER BY d.todo_index"))?;
        let rows = query.query_map(params![workspace, plan_id], |row| {
            Ok((
                row.get::<_, usize>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (todo_index, choice, device, operation, previous_device, previous_operation) = row?;
            let choice: PlanTodoChoice = serde_json::from_str(&choice)?;
            let (mut receipt, previous_owner, _) = read(&connection, &device, &operation)?
                .filter(|(receipt, _, _)| receipt.workspace_id == workspace)
                .ok_or_else(|| invalid("todo command missing"))?;
            recover(&mut receipt, &previous_owner, owner);
            let mut queued = queue::read_queue(&connection, &device, &operation)?;
            let can_retry = retryable(&choice, &receipt, queued.as_ref(), owner);
            if let Some(queued) = &mut queued {
                queue::recover_owner(queued, owner);
            }
            Ok(StoredTodoDecision {
                todo_index,
                choice,
                can_retry,
                retry_of: previous_device.zip(previous_operation).map(
                    |(device_id, operation_id)| TodoDecisionRef {
                        device_id,
                        operation_id,
                    },
                ),
                receipt,
                queued,
            })
        })
        .collect()
    }
}

fn retryable(
    choice: &PlanTodoChoice,
    receipt: &MobileCommandReceipt,
    queued: Option<&QueuedMessage>,
    owner: &str,
) -> bool {
    match choice {
        PlanTodoChoice::Accept { queue: true, .. } => queued.is_some_and(|q| {
            q.state == "cancelled" || (q.state == "queued" && q.request.owner_id != owner)
        }),
        PlanTodoChoice::Accept { queue: false, .. } => receipt.state == "rejected",
        PlanTodoChoice::Reject => false,
    }
}

type AttemptSource = (String, usize, String, Option<TodoDecisionRef>);
fn attempt(
    connection: &Connection,
    device: &str,
    operation: &str,
) -> StorageResult<Option<AttemptSource>> {
    Ok(connection.query_row(
        &format!("SELECT plan_id,todo_index,choice_json,previous_device_id,previous_operation_id FROM ({ATTEMPTS}) WHERE device_id=?1 AND operation_id=?2"),
        params![device,operation], |row| {
            let device: Option<String> = row.get(3)?;
            let operation: Option<String> = row.get(4)?;
            Ok((row.get(0)?,row.get(1)?,row.get(2)?,device.zip(operation).map(|(device_id,operation_id)|TodoDecisionRef {device_id,operation_id})))
        }).optional()?)
}
