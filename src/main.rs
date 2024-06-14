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

use minicbor::decode::info;
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
use limero::Source;
use limero::SinkTrait;

#[derive(Clone)]
struct Data {
    i32: i32,
    i64: i64,
    f32: f32,
    s: String,
}

#[derive( Clone)]
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
                        if  data.i32 % 100000 == 0 {
                            info!("Pinger received Pong {} ",data.i32);};
                        if data.i32 == 1_000_000 {
                            info!("Pinger received Pong {} stopping.. ",data.i32);
                            break;
                        }
                        self.ponger.push(PingPongMsg::Ping { src:self.sink.sink_ref(),data });

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
    pub fn sink_ref(&self)->SinkRef<PingPongMsg> {
        self.sink.sink_ref()
    }
    async fn run(&mut self) {
        info!("Ponger started");

        loop {
            let x = self.sink.read().await;
            if let Some(x) = x {
                match x {
                    PingPongMsg::Ping { src,data } => {
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





// external interface
pub enum PubSubEvent {
    Publish { topic: String, payload: String },
    Connected,
    Disconnected,
}
pub enum PubSubCmd {
    Publish { topic: String, payload: String },
}

pub enum LinkEvent {
    Connected,
    Disconnected,
}

struct PubSub {
    events: Source<PubSubEvent>,
    queue: Sink<PubSubCmd>,
    link: SinkRef<LinkEvent>,
    state: PubSubState,
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

#[tokio::main(worker_threads = 2)]
async fn main() {
    logger::init();
    let mut ponger = Ponger::new();
    let mut pinger = Pinger::new(ponger.sink_ref());
    let pinger_task = tokio::spawn(async move {
        pinger.run().await;
    });
    let ponger_task = tokio::spawn(async move {
        ponger.run().await;
    });
    let _ = tokio::join!(ponger_task, pinger_task);
    // sleep
    tokio::time::sleep(Duration::from_secs(1000)).await;
}
