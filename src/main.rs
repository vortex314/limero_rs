#![allow(unused_imports)]
#![allow(dead_code)]

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

use limero::Flow;
use limero::FlowFunction;
use limero::FlowMap;
use limero::SourceTrait;
use minicbor::decode::info;
use tokio::select;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

use core::result::Result;
use std::marker::PhantomData;
use std::sync::Mutex as StdMutex;
use std::sync::RwLock;
use std::u64::MAX;
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
    pub fn sink(&self) -> SinkRef<PingPongMsg> {
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

#[derive(Clone, Debug)]
enum LedCmd {
    On,
    Off,
    Toggle,
    Blink { msec: u64 },
    Pulse { msec: u64},
}

#[derive(PartialEq,Debug)]
enum LedState {
    On,
    Off,
    Blinking ,
    Pulsing ,
}
struct Led {
    sink: Sink<LedCmd>,
    state: LedState,
    timeout: Duration
}

impl Led {
    pub fn new() -> Self {
        Led {
            sink: Sink::new(3),
            state: LedState::Off,
            timeout: Duration::from_millis(std::u64::MAX)
        }
    }
    fn sink(&self) -> SinkRef<LedCmd> {
        self.sink.sink_ref()
    }
    fn set_state (&mut self,new_state : LedState)  {
        info!("Led state {:?}", new_state);
        self.state = new_state;
    }
    async fn run(&mut self) {
        info!("Led started");
        loop {
            select! {
                _ = tokio::time::sleep(self.timeout) => {
                    info!("Led received Timeout");
                    if self.state == LedState::Blinking {
                        self.set_state(LedState::On);
                    } else if self.state == LedState::Pulsing {
                        self.set_state(LedState::Off);
                        self.timeout = Duration::from_millis(std::u64::MAX);
                    }
                }
                led_cmd = self.sink.read() => {
                    info!("Led received {:?}", led_cmd);
                    if let Some(led_cmd) = led_cmd {
                    match led_cmd {
                        LedCmd::On => {
                            info!("Led received On");
                            self.set_state(LedState::On);
                            self.timeout = Duration::from_millis(std::u64::MAX);
                        }
                        LedCmd::Off => {
                            info!("Led received Off");
                            self.set_state(LedState::Off);
                            self.timeout = Duration::from_millis(std::u64::MAX);
                        }
                        LedCmd::Toggle => {
                            info!("Led received Toggle");
                            self.set_state(if self.state == LedState::Off { LedState::On } else { LedState::Off });
                            self.timeout = Duration::from_millis(std::u64::MAX);
                        }
                        LedCmd::Blink { msec } => {
                            info!("Led received Blink");
                            self.set_state(LedState::Blinking);
                            self.timeout = Duration::from_millis(msec);
                        }
                        LedCmd::Pulse { msec } => {
                            info!("Led received Pulse");
                            self.set_state(LedState::Pulsing);
                            self.timeout = Duration::from_millis(msec);
                        }
                    }
                }
            }
            }
        }
    }
}

impl SinkTrait<LedCmd> for Led {
    fn push(&self, _message: LedCmd) {
        self.sink.push(_message); 
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
    pub fn sink(&self) -> Box<dyn SinkTrait<LinkCmd>> {
        Box::new(self.sink.sink_ref())
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

impl SourceTrait<LinkEvent> for Link {
    fn add_listener(&mut self, sink: Box<dyn SinkTrait<LinkEvent>>) {
        self.events.add_listener(sink);
    }
}

fn link_recv_to_led_pulse(link_event: LinkEvent) -> Option<LedCmd> {
    match link_event {
        LinkEvent::Recv { payload: _ } => Some(LedCmd::Pulse { msec: 100 }),
        _ => None,
    }
}

fn via<T>( src:&mut dyn SourceTrait<T>,sink : Box<dyn SinkTrait<T>>) {
    src.add_listener(sink);
}

fn connect<T,U>(src: &mut dyn SourceTrait<T>, func : fn(T)->Option<U>, sink: SinkRef<U>) where T:Clone+Send+Sync+'static, U:Clone+Send+Sync+'static {
    let mut flow = FlowFunction::new(func);
    flow.add_listener(Box::new(sink));
    src.add_listener(Box::new(flow));
}

#[tokio::main(worker_threads = 1)]
async fn main() {
    logger::init();
    let mut ponger = Ponger::new();
    let mut pinger = Pinger::new(ponger.sink());
    let mut link = Link::new();
    let mut pubsub = PubSub::new(link.sink());
    let mut led = Led::new();
    link.add_listener(pubsub.link_sink());

    let ff = FlowFunction::new(link_recv_to_led_pulse);

    &mut link  >> ff ; //ff>> led.sink();

    connect(&mut link, link_recv_to_led_pulse, led.sink());
    connect(&mut link, |_x| { Some(LedCmd::Pulse{ msec:100}) } , led.sink());

    select! {
            _ = ponger.run() => {}
            _ = pinger.run() => {}
            _ = link.run() => {}
            _ = pubsub.run() => {}
            _ = led.run() => {}

    }
}
