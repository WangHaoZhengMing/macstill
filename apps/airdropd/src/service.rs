//! Session-bus API; commands are acknowledged only after queueing.
use super::*;
pub(super) struct Api {
    pub(super) state: Shared,
    pub(super) commands: mpsc::Sender<Command>,
}
#[zbus::interface(name = "org.macstill.AirDrop1")]
impl Api {
    fn get_snapshot(&self) -> String {
        serde_json::to_string(&*self.state.lock().unwrap()).unwrap()
    }
    async fn execute(&self, json: String) -> zbus::fdo::Result<()> {
        if json.len() > 65536 {
            return Err(zbus::fdo::Error::InvalidArgs("Request too large".into()));
        }
        let cmd: Command = serde_json::from_str(&json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        self.commands
            .try_send(cmd)
            .map_err(|_| zbus::fdo::Error::Failed("AirDrop is busy".into()))?;
        Ok(())
    }
}
