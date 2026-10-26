//! Per-user AirDrop actor. The upstream owlyd is the only privileged process.
mod bluez;
mod discovery;
mod service;
use discovery::{discover, peer_address};
use service::Api;
mod notifications;
mod session;
use anyhow::{Context, Result};
use macstill_airdrop::{Command, Peer, Snapshot, Transfer, Visibility};
use owly_airdrop::{
    driver::{
        mdns::Mdns,
        receiver::{Options, Receiver, ReceiverEvent, Request},
        sender::{Event, Payload, Sender},
        tls::Identity,
    },
    proto::{
        mdns::{BrowseEvent, Burst, RECEIVER_FLAGS, SUPPORTS_DVZIP, Service},
        messages::Offer,
        receiver::Config,
        sender::{self, Outcome},
    },
};
use session::Session;
use std::{
    collections::HashMap,
    net::{Ipv6Addr, SocketAddrV6},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};
use zbus::zvariant::OwnedValue;
const RADIO_NAME: &str = "io.github.t4t5.Owly";
const RADIO_PATH: &str = "/io/github/t4t5/Owly";
const RADIO_IFACE: &str = "io.github.t4t5.Owly.Radio1";
type Shared = Arc<Mutex<Snapshot>>;
fn change(state: &Shared, f: impl FnOnce(&mut Snapshot)) {
    let mut s = state.lock().unwrap();
    f(&mut s);
    s.revision = s.revision.wrapping_add(1);
}
fn edit_transfer(state: &Shared, id: &str, f: impl FnOnce(&mut Transfer)) {
    change(state, |s| {
        if let Some(t) = s.transfers.iter_mut().find(|t| t.id == id) {
            f(t)
        }
    })
}
fn record(state: &Shared, t: Transfer) {
    change(state, |s| {
        if s.transfers.len() >= 100 {
            if let Some(i) = s.transfers.iter().position(|t| {
                ["completed", "failed", "declined", "cancelled", "expired"]
                    .contains(&t.state.as_str())
            }) {
                s.transfers.remove(i);
            }
        }
        s.transfers.push(t);
    });
}
async fn radio<'a>(c: &'a zbus::Connection) -> Result<zbus::Proxy<'a>> {
    Ok(zbus::proxy::Builder::new(c)
        .destination(RADIO_NAME)?
        .path(RADIO_PATH)?
        .interface(RADIO_IFACE)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?)
}
async fn system_connection() -> Result<zbus::Connection> {
    // An explicit, unprivileged integration-test bus; never changes a system radio.
    let address = std::env::var("MACSTILL_AIRDROP_RADIO_BUS").ok();
    let builder = if let Some(ref address) = address {
        zbus::connection::Builder::address(address.as_str())?
    } else {
        zbus::connection::Builder::system()?
    };
    Ok(builder
        .method_timeout(Duration::from_secs(180))
        .build()
        .await?)
}
async fn probe(state: &Shared) {
    let result = async {
        let c = system_connection().await?;
        let json: String = radio(&c).await?.call("Probe", &()).await?;
        Ok::<_, anyhow::Error>(serde_json::from_str::<serde_json::Value>(&json)?)
    }
    .await;
    change(state, |s| match result {
        Ok(v) => {
            s.available = v["hardware"].as_bool() == Some(true)
                && v["missing"].as_array().is_some_and(|v| v.is_empty());
            s.mode = v["mode"].as_str().unwrap_or("").into();
            s.warning=(s.mode!="shared"&&!s.mode.is_empty()).then(||"This Wi-Fi adapter pauses its normal Wi-Fi connection while AirDrop is enabled.".into());
            if v["status"].as_str().is_some_and(|s| s != "tested") {
                let note = "Experimental Wi-Fi adapter support: compatibility with this device has not been confirmed.";
                s.warning = Some(
                    s.warning
                        .take()
                        .map(|w| format!("{w} {note}"))
                        .unwrap_or_else(|| note.into()),
                );
            }
            s.error = if s.available {
                None
            } else {
                Some(
                    v["missing"]
                        .as_array()
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|v| v["say"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "This Wi-Fi adapter is not supported by Owly.".into()),
                )
            };
        }
        Err(e) => {
            s.available = false;
            s.error = Some(format!(
                "Owly radio service is unavailable: {e}. Install and start owlyd."
            ));
        }
    });
}
fn service_id() -> Result<String> {
    let mut b = [0; 6];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(b.iter().map(|b| format!("{b:02x}")).collect())
}
fn state_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state")
        })
        .join("macstill/airdrop")
}
enum Internal {
    Incoming(String, ReceiverEvent),
    Peer(Peer, SocketAddrV6, u32),
    Lost(String),
    Progress(String, Event),
    Finished(String, Result<Outcome, String>),
}
async fn run(state: Shared, mut commands: mpsc::Receiver<Command>, session_bus: zbus::Connection) {
    let (events_tx, mut events) = mpsc::unbounded_channel();
    let mut session: Option<Session> = None;
    let mut requests = HashMap::<String, Request>::new();
    let mut peers = HashMap::<String, (SocketAddrV6, u32, Instant)>::new();
    let mut sends = HashMap::<String, JoinHandle<()>>::new();
    let mut timer = tokio::time::interval(Duration::from_secs(2));
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    probe(&state).await;
    loop {
        tokio::select! {
        _=session_bus.closed()=>break,
        _=tokio::signal::ctrl_c()=>break,
        _=terminate.recv()=>break,
        command=commands.recv()=>{let Some(command)=command else{break};
        let command = match command {
            Command::SetVisibility { visibility: Visibility::Off } => Command::Disable,
            Command::SetVisibility { visibility: Visibility::EveryoneTenMinutes } => Command::Enable { seconds: 600 },
            Command::SetVisibility { visibility: Visibility::Everyone } => Command::Enable { seconds: 0 },
            other => other,
        };
        match command{
        Command::SetVisibility { .. } => unreachable!(),
        Command::SetReceiveDirectory { path } => {
            if session.is_some() { change(&state, |s| s.error=Some("Turn off AirDrop before changing the download folder.".into())); continue; }
            if !path.is_absolute() { change(&state, |s| s.error=Some("Choose an absolute directory path.".into())); continue; }
            let result = async { tokio::fs::create_dir_all(&path).await?; tokio::fs::create_dir_all(state_dir()).await?;
                let data = serde_json::to_vec(&path)?;
                let temporary = state_dir().join("receive-directory.new");
                tokio::fs::write(&temporary, data).await?;
                tokio::fs::rename(temporary, state_dir().join("receive-directory.json")).await?;
                Ok::<_, anyhow::Error>(()) }.await;
            change(&state, |s| match result { Ok(()) => { s.receive_directory=path; s.error=None; }, Err(e) => s.error=Some(e.to_string()) });
        },
        Command::Refresh=>{if session.is_none(){probe(&state).await;}},
        Command::Enable{seconds}=>{if state.lock().unwrap().state=="finishing"{change(&state,|s|s.error=Some("Wait for current transfers to finish before restarting discovery.".into()));continue;}if session.is_none(){change(&state,|s|{s.state="starting".into();s.error=None;});match Session::start(&state,events_tx.clone(),seconds).await{Ok(active)=>{let until=active.until;session=Some(active);change(&state,|s|{s.available=true;s.state="on".into();s.expires_at=until;s.visibility=if until==0 {Visibility::Everyone}else{Visibility::EveryoneTenMinutes};});},Err(e)=>{change(&state,|s|{s.state="off".into();s.visibility=Visibility::Off;s.error=Some(e.to_string());});}}}else if let Some(s)=session.as_mut(){s.until=if seconds==0 {0}else{macstill_airdrop::now()+u64::from(seconds.min(600))};change(&state,|p|{p.expires_at=s.until;p.visibility=if seconds==0 {Visibility::Everyone}else{Visibility::EveryoneTenMinutes};});}},
        Command::Disable=>{if sends.is_empty()&&!state.lock().unwrap().transfers.iter().any(|t|t.incoming&&t.state=="receiving"){requests.clear();if let Some(s)=session.take(){change(&state,|s|s.state="stopping".into());s.stop().await;}peers.clear();change(&state,|s|{s.state="off".into();s.visibility=Visibility::Off;s.expires_at=0;s.peers.clear();for t in &mut s.transfers{if t.state=="pending"{t.state="declined".into();}}});}else{change(&state,|s|s.error=Some("Finish or cancel current transfers before turning AirDrop off.".into()));}},
        Command::Answer{id,accept}=>{if let Some(r)=requests.remove(&id){let allowed=session.as_ref().is_some_and(|s|s.until==0||macstill_airdrop::now()<s.until);if accept&&allowed{r.accept();edit_transfer(&state,&id,|t|t.state="receiving".into());}else{r.decline();edit_transfer(&state,&id,|t|t.state="declined".into());}}},
        Command::Send{peer,files}=>{
         if session.is_none()||!state.lock().unwrap().enabled(){change(&state,|s|s.error=Some("Enable AirDrop before sending.".into()));continue}
         if sends.len()>=4{change(&state,|s|s.error=Some("Wait for one of the current transfers to finish.".into()));continue}
         let Some((addr,flags,_))=peers.get(&peer).cloned()else{change(&state,|s|s.error=Some("This device is no longer nearby.".into()));continue};
         if files.is_empty()||files.len()>128||files.iter().any(|p|!p.is_absolute()||!p.exists()){change(&state,|s|s.error=Some("Select existing files or folders (up to 128).".into()));continue}
         let id=format!("send-{}",service_id().unwrap_or_default());let name=state.lock().unwrap().peers.iter().find(|p|p.id==peer).map(|p|p.name.clone()).unwrap_or(peer);
         record(&state,Transfer{id:id.clone(),peer:name,incoming:false,files:files.iter().map(|p|p.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect(),state:"preparing".into(),sent:0,total:0,error:None});
         let e=events_tx.clone();let tid=id.clone();let task=tokio::spawn(async move{
          let result=async{let sender=Sender::new(sender::Config::new("Macstill",service_id()?))?;let(tx,mut rx)=mpsc::unbounded_channel();let events=e.clone();let progress_id=tid.clone();let reporter=tokio::spawn(async move{while let Some(event)=rx.recv().await{let _=events.send(Internal::Progress(progress_id.clone(),event));}});let result=sender.send(addr,&Payload::Files(files),flags,Some(tx)).await;let _=reporter.await;Ok::<_,anyhow::Error>(result?)}.await.map_err(|e|e.to_string());
          let _=e.send(Internal::Finished(tid,result));
         });sends.insert(id,task);
        },
        Command::Cancel{id}=>{if let Some(t)=sends.remove(&id){t.abort();edit_transfer(&state,&id,|t|t.state="cancelled".into());}else if let Some(r)=requests.remove(&id){r.decline();edit_transfer(&state,&id,|t|t.state="declined".into());}},
        }},
        event=events.recv()=>{let Some(event)=event else{break};match event{
        Internal::Peer(peer,addr,flags)=>{if session.is_none()||(!peers.contains_key(&peer.id)&&peers.len()>=256){continue}peers.insert(peer.id.clone(),(addr,flags,Instant::now()));change(&state,|s|{s.peers.retain(|p|p.id!=peer.id);s.peers.push(peer);});},
        Internal::Lost(id)=>{peers.remove(&id);change(&state,|s|s.peers.retain(|p|p.id!=id));},
        Internal::Progress(id,event)=>edit_transfer(&state,&id,|t|match event{Event::Accepted=>t.state="sending".into(),Event::Progress(p)=>{t.sent=p.sent;t.total=p.total;t.state="sending".into();}}),
        Internal::Finished(id,result)=>{sends.remove(&id);edit_transfer(&state,&id,|t|match result{Ok(Outcome::Delivered)=>t.state="completed".into(),Ok(Outcome::Declined)=>t.state="declined".into(),Ok(Outcome::Failed(e))|Err(e)=>{t.state="failed".into();t.error=Some(e);}});},
        Internal::Incoming(epoch,event)=>match event{
        ReceiverEvent::Request(request)=>{if session.as_ref().is_none_or(|s|s.until!=0 && macstill_airdrop::now()>=s.until){request.decline();continue}let id=format!("receive-{epoch}-{}",request.id);let files=match &request.ask.offer{Offer::Files(files)=>files.iter().map(|f|f.name.clone()).collect(),Offer::Urls(urls)=>urls.clone()};record(&state,Transfer{id:id.clone(),peer:request.ask.sender_computer_name.clone(),incoming:true,files,state:"pending".into(),sent:0,total:0,error:None});requests.insert(id,request);},
        ReceiverEvent::Withdrawn{id}=>{let id=format!("receive-{epoch}-{id}");requests.remove(&id);edit_transfer(&state,&id,|t|t.state="expired".into());},
        ReceiverEvent::Received{id,paths}=>{edit_transfer(&state,&format!("receive-{epoch}-{id}"),|t|{t.state="completed".into();t.files=paths.iter().map(|p|p.display().to_string()).collect();});},
        ReceiverEvent::Urls{id,urls}=>{edit_transfer(&state,&format!("receive-{epoch}-{id}"),|t|{t.state="completed".into();t.files=urls;});},
        ReceiverEvent::Failed{id,reason}=>edit_transfer(&state,&format!("receive-{epoch}-{id}"),|t|{t.state="failed".into();t.error=Some(reason);}),
        _=>{}
        }
        }},
        _=timer.tick()=>{
         sends.retain(|_,t|!t.is_finished());
         if let Some(active)=session.as_ref(){
          let radio=radio(&active.connection).await;let lost;
          if let Ok(radio)=radio{let live=radio.get_property::<u64>("Lease").await.unwrap_or(0);lost=live!=active.lease;
           let list=radio.get_property::<Vec<HashMap<String,OwnedValue>>>("Peers").await.unwrap_or_default();
           for (id,(addr,flags,_)) in peers.iter().take(64){let e=events_tx.clone();let (id,addr,flags)=(id.clone(),*addr,*flags);tokio::spawn(async move{discover(addr,id,flags,e).await;});}

           for p in list.into_iter().take(64){if let Some(mac)=p.get("mac").and_then(|v|v.downcast_ref::<String>().ok()){if let Some(addr)=peer_address(&mac,active.index){let e=events_tx.clone();tokio::spawn(async move{discover(addr,mac,RECEIVER_FLAGS|SUPPORTS_DVZIP,e).await;});}}}
          }else{lost=true}
          let expired=active.until!=0 && macstill_airdrop::now()>=active.until;
          let receiving=state.lock().unwrap().transfers.iter().any(|t|t.incoming&&t.state=="receiving");
          if expired&&!lost{requests.clear();change(&state,|s|{s.visibility=Visibility::Off;for t in &mut s.transfers{if t.state=="pending"{t.state="expired".into();}}});if let Some(s)=session.as_mut(){s._registration.take();s._advert.take();}change(&state,|s|{s.state="finishing".into();s.expires_at=0;});}
          if lost||(expired&&sends.is_empty()&&!receiving){requests.clear();if let Some(s)=session.take(){s.stop().await;}peers.clear();change(&state,|s|{s.state="off".into();s.visibility=Visibility::Off;s.peers.clear();s.expires_at=0;if lost{s.error=Some("The AirDrop radio connection was lost.".into());for t in &mut s.transfers{if ["pending","receiving"].contains(&t.state.as_str()){t.state="failed".into();t.error=Some("Radio connection lost".into());}}}});}
         }
         let expired:Vec<_>=peers.iter().filter(|(_,(_,_,seen))|seen.elapsed()>Duration::from_secs(12)).map(|(id,_)|id.clone()).collect();for id in expired{peers.remove(&id);change(&state,|s|s.peers.retain(|p|p.id!=id));}
        }
        }
    }
    for (_, task) in sends {
        task.abort()
    }
    requests.clear();
    if let Some(s) = session {
        s.stop().await;
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let state = Arc::new(Mutex::new(Snapshot {
        state: "off".into(),
        receive_directory: std::fs::read(state_dir().join("receive-directory.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PathBuf>(&bytes).ok())
            .filter(|path| path.is_absolute())
            .unwrap_or_else(macstill_airdrop::downloads),
        ..Default::default()
    }));
    let (tx, rx) = mpsc::channel(32);
    let _connection = zbus::connection::Builder::session()?
        .name(macstill_airdrop::NAME)?
        .serve_at(
            macstill_airdrop::PATH,
            Api {
                state: state.clone(),
                commands: tx.clone(),
            },
        )?
        .build()
        .await?;
    let notifications = tokio::spawn(notifications::run(state.clone(), tx, _connection.clone()));
    run(state, rx, _connection.clone()).await;
    notifications.abort();
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_peer_address_to_radio_interface() {
        let a = peer_address("00:11:22:33:44:55", 7).unwrap();
        assert_eq!(a.scope_id(), 7);
        assert_eq!(a.ip().to_string(), "fe80::211:22ff:fe33:4455");
        assert!(peer_address("bad", 7).is_none());
    }
}
