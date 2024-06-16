#![allow(unused_imports)]
#![allow(dead_code)]
#[cfg(all(feature = "std", feature = "no_std"))]
compile_error!("feature \"std\" and feature \"no_std\" cannot be enabled at the same time");

#[cfg(feature = "tokio")]
use {
    std::borrow::BorrowMut,
    std::cell::RefCell,
    std::collections::BTreeMap,
    std::io::Write,
    std::pin::pin,
    std::rc::Rc,
    std::sync::Arc,
    std::thread::sleep,
    std::time::{Duration, Instant},
    std::vec::Vec,
    std::{ops::Shr, pin::Pin},
};

use limero::SinkFunction;
use minicbor::decode::info;
use tokio::select;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

use core::result::Result;
use std::marker::PhantomData;
use std::sync::Mutex as StdMutex;
use std::sync::RwLock;
mod logger;
use log::error;
use log::{debug, info};
use std::error::Error;

mod limero;
use limero::Sink;
use limero::SinkRef;
use limero::SinkTrait;
use limero::Src;

#[derive(Clone)]
struct Data {
    i32: i32,
    i64: i64,
    f32: f32,
    s: String,
}

#[derive(Clone)]
enum PingPongMsg {
    Ping {
        src: SinkRef<PingPongMsg>,
        data: Data,
    },
    Pong {
        data: Data,
    },
}

struct Pinger {
    sink: Sink<PingPongMsg>,
    ponger: SinkRef<PingPongMsg>,
}

impl Pinger {
    pub fn new(ponger: SinkRef<PingPongMsg>) -> Self {
        Pinger {
            sink: Sink::new(5),
            ponger,
        }
    }

    async fn run(&mut self) {
        info!("Pinger started");
        self.ponger.push(PingPongMsg::Ping {
            src: self.sink.sink_ref(),
            data: Data {
                i32: 0,
                i64: 0,
                f32: 0.0,
                s: "Hello".to_string(),
            },
        });

        loop {
            let x = self.sink.read().await;
            if let Some(x) = x {
                match x {
                    PingPongMsg::Pong { mut data } => {
                        data.i32 += 1;
                        if data.i32 % 100000 == 0 {
                            info!("Pinger received Pong {} ", data.i32);
                        };
                        if data.i32 == 1_000_000 {
                            info!("Pinger received Pong {} stopping.. ", data.i32);
                            break;
                        }
                        self.ponger.push(PingPongMsg::Ping {
                            src: self.sink.sink_ref(),
                            data,
                        });
                    }
                    _ => {}
                }
            } else {
                info!("Pinger received None");
            }
        }
    }
}

struct Ponger {
    sink: Sink<PingPongMsg>,
}

impl Ponger {
    pub fn new() -> Self {
        Ponger { sink: Sink::new(1) }
    }
    pub fn sink_ref(&self) -> SinkRef<PingPongMsg> {
        self.sink.sink_ref()
    }
    async fn run(&mut self) {
        info!("Ponger started");

        loop {
            let x = self.sink.read().await;
            if let Some(x) = x {
                match x {
                    PingPongMsg::Ping { src, data } => {
                        src.push(PingPongMsg::Pong { data });
                    }
                    _ => {}
                }
            } else {
                info!("Ponger received None");
            }
        }
    }
}

#[derive(Clone)]
enum LedCmd {
    On,
    Off,
    Toggle,
    Blink { duration: u64 },
    Pulse { duration: u64 },
}

struct Led {
    state: bool,
}

impl Led {
    pub fn new() -> Self {
        Led { state: false }
    }
    async fn run() {
        info!("Led started");
    }
}

impl SinkTrait<LedCmd> for Led {
    fn push(&self, _message: LedCmd) {
        
    }
}



// external interface
#[derive(Clone)]
pub enum PubSubEvent {
    Publish { topic: String, payload: String },
    Connected,
    Disconnected,
}
#[derive(Clone)]

pub enum PubSubCmd {
    Publish { topic: String, payload: String },
}

struct PubSub {
    events: Src<PubSubEvent>,
    cmd_sink: Sink<PubSubCmd>,
    link: Box<dyn SinkTrait<LinkCmd>>,
    link_events: Sink<LinkEvent>,
    state: PubSubState,
}

impl PubSub {
    pub fn new(link: Box<dyn SinkTrait<LinkCmd>>) -> Self {
        PubSub {
            events: Src::new(),
            cmd_sink: Sink::new(5),
            link,
            link_events: Sink::new(5),
            state: PubSubState::Disconnected,
        }
    }

    pub fn events(&self) -> &Src<PubSubEvent> {
        &self.events
    }

    pub fn link_sink(&self) -> Box<dyn SinkTrait<LinkEvent>> {
        Box::new(self.link_events.sink_ref())
    }

    pub fn sink(&self) -> Box<dyn SinkTrait<PubSubCmd>> {
        Box::new(self.cmd_sink.sink_ref())
    }

    pub async fn run(&mut self) {
        info!("PubSub started");
        self.link.push(LinkCmd::Connect);
        loop {
            select! {
                link_event = self.link_events.read() => {
                    info!("PubSub received LinkEvent");
                    if let Some(link_event) = link_event {
                    match link_event {
                        LinkEvent::Connected => {
                            info!("PubSub received Connected");
                            self.state = PubSubState::Connected;
                            self.events.emit(PubSubEvent::Connected);
                            self.link.push(LinkCmd::Send { payload: vec![1, 2, 3] });
                        }
                        LinkEvent::Disconnected => {
                            info!("PubSub received Disconnected");
                            self.state = PubSubState::Disconnected;
                            self.events.emit(PubSubEvent::Disconnected);
                        }
                        LinkEvent::Recv { payload : _} => {
                            info!("PubSub received Recv");
                        }
                    }
                }}
                pubsub_cmd = self.cmd_sink.read() => {
                    info!("PubSub received Cmd");
                    if let Some(pubsub_cmd) = pubsub_cmd {
                    match pubsub_cmd {
                        PubSubCmd::Publish { topic, payload } => {
                            info!("PubSub received Publish {} {}", topic, payload);
                            self.events.emit(PubSubEvent::Publish { topic, payload });
                        }

                    }
                }
                }
            }
        }
    }
}

enum PubSubState {
    Disconnected,
    Connected,
}

pub enum PubSubWire {
    Publish { id: u16, payload: String },
    Subscribe { topic: String },
    SubAck,
    Register { topic: String, id: u16 },
    RegAck { id: u16 },
    Connect,
    Disconnect,
}

#[derive(Clone)]
pub enum LinkEvent {
    Connected,
    Disconnected,
    Recv { payload: Vec<u8> },
}
#[derive(Clone)]
pub enum LinkCmd {
    Connect,
    Disconnect,
    Send { payload: Vec<u8> },
}
struct Link {
    events: Src<LinkEvent>,
    sink: Sink<LinkCmd>,
}

impl Link {
    pub fn new() -> Self {
        Link {
            events: Src::new(),
            sink: Sink::new(5),
        }
    }
    pub fn add_listener(&mut self, sink: Box<dyn SinkTrait<LinkEvent>>) {
        self.events.add_listener(sink);
    }
    pub fn cmd_sink(&self) -> Box<dyn SinkTrait<LinkCmd>> {
        Box::new(self.sink.sink_ref())
    }

    pub fn events(&self) -> &Src<LinkEvent> {
        &self.events
    }

    async fn run(&mut self) {
        info!("Link started");
        loop {
            let x = self.sink.read().await;
            if let Some(x) = x {
                match x {
                    LinkCmd::Connect => {
                        info!("Link received Connect");
                        self.events.emit(LinkEvent::Connected);
                    }
                    LinkCmd::Disconnect => {
                        info!("Link received Disconnect");
                        self.events.emit(LinkEvent::Disconnected);
                    }
                    LinkCmd::Send { payload } => {
                        info!("Link received Send");
                        self.events.emit(LinkEvent::Recv { payload });
                    }
                }
            } else {
                info!("Link received None");
            }
        }
    }
}

fn map_recv_to_pulse(link_event: LinkEvent) -> Option<LedCmd>  {
    match link_event {
        LinkEvent::Recv { payload: _ } => {
            Some(LedCmd::Pulse { duration: 1000 })
        }
        _ => {
            None
        }
    }
}

#[tokio::main(worker_threads = 1)]
async fn main() {
    logger::init();
    let mut ponger = Ponger::new();
    let mut pinger = Pinger::new(ponger.sink_ref());
    let mut link = Link::new();
    let mut pubsub = PubSub::new(link.cmd_sink());
    let led = Led::new();
    link.add_listener(pubsub.link_sink());
    let mapper= SinkFunction::new (Box::new(map_recv_to_pulse),Box::new(led));
    link.events().add_listener(Box::new(mapper));
    select! {
            _ = ponger.run() => {}
            _ = pinger.run() => {}
            _ = link.run() => {}
            _ = pubsub.run() => {}

    }
}
