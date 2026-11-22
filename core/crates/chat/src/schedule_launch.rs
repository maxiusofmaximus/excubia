//! Starting a scheduled run (F4-CORE): the store has written the run as started; the seat
//! starts its delegation, and the run is settled with the delegation it got, or as failed.

use crate::model_team::Delegation;
use crate::seat::{ChatSeat, ToolError};
use crate::store::{ChatStore, Launch};

pub async fn launch_schedule<S: ChatSeat + ?Sized>(
    chat: &ChatStore,
    seat: &S,
    launch: Launch,
) -> Result<Delegation, ToolError> {
    let run_id = launch.run_id.clone();
    match seat.run_scheduled(launch).await {
        Ok(delegation) => {
            if let Err(error) = chat.settle_schedule_run(&run_id, Some(&delegation.id)) {
                eprintln!("host: could not settle a scheduled run: {error}");
            }
            Ok(delegation)
        }
        Err(error) => {
            if let Err(settle) = chat.settle_schedule_run(&run_id, None) {
                eprintln!("host: could not settle a scheduled run: {settle}");
            }
            Err(error)
        }
    }
}
