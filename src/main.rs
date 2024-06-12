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

use limero::ActorTrait;
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
use limero::pre_process;
use limero::ActorRef;
use limero::Emitter;
use limero::Sink;
use limero::Source;

struct Request<T, U> {
    src: ActorRef<U>,
    data: T,
}

enum MasterMsg {
    Request { src: ActorRef<Msg>, data: Msg },
    Response { data: Msg },
}

enum MasterEvent {
    Event { time: u64 },
}

struct Master {
    sender: Sender<MasterMsg>,
    receiver: Receiver<MasterMsg>,
    emitter: Emitter<MasterEvent>,
}

impl Master {
    pub fn new() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(100);
        Master {
            sender,
            receiver,
            emitter: Emitter::new(),
        }
    }
    async fn run(&mut self) {
        info!("Master started");
        loop {
            let x = self.receiver.recv().await;
            if let Some(mut x) = x {
                match x {
                    MasterMsg::Request { src, data } => {
                        data.i32 += 1;
                        src.tell(data);
                    }
                    MasterMsg::Response { data } => {
                        if data.i32 % 100000 == 0 {
                            info!("Master sent : {:?}", data);
                        }
                    }
                }
            } else {
                info!("Master received None");
            }
        }
    }
}

impl ActorTrait<MasterMsg, MasterEvent> for Master {
    async fn run() {
        info!("Master started");
        let mut requests = BTreeMap::new();
        let mut id = 0;
        loop {
            let x = self.sink.read().await;
            if let Some(mut x) = x {
                x.i32 += 1;
                self.source.emit(&x);
                if x.i32 % 100000 == 0 {
                    info!("Master sent : {:?}", x);
                }
            } else {
                info!("Master received None");
            }
        }
    }
    fn actor_ref(&self) -> ActorRef<MasterMsg> {
        ActorRef::new(self.sink.sender.clone())
    }
    fn add_listener(&self, listener: ActorRef<MasterMsg>) {
        self.source.add_sink(listener);
    }
}

#[derive(Clone, Debug)]
struct Msg {
    pub i32: i32,
    pub i64: i64,
    pub f32: f32,
    pub s: String,
}

struct Echo {
    pub sink: Sink<Msg>,
    pub source: Source<Msg>,
}

impl Echo {
    pub fn new() -> Self {
        Echo {
            sink: Sink::new(),
            source: Source::new(),
        }
    }
    async fn run(&mut self) {
        info!("Echo started");

        loop {
            let x = self.sink.read().await;
            if let Some(x) = x {
                self.source.emit(&x);
            } else {
                info!("Echo received None");
            }
        }
    }
}

fn mapper(x: Msg) -> Option<Msg> {
    Some(Msg {
        i32: x.i32 + 1,
        i64: x.i64 + 1,
        f32: x.f32 + 1.0,
        s: x.s.clone(),
    })
}

#[tokio::main(worker_threads = 2)]
async fn main() {
    logger::init();

    let mut master = Master::new();
    let mut echo = Echo::new();
    //  &master.source >> &echo.sink;
    &echo.source >> &master.sink;
    // let _snk = pre_process(|x| { Some(x) }, master.sink.sink());
    &master.source >> pre_process(mapper, echo.sink.sink());

    master.source.emit(&Msg {
        i32: 0,
        i64: 0,
        f32: 0.0,
        s: "Hello".to_string(),
    });

    let master_task = tokio::spawn(async move {
        master.run().await;
    });
    let echo_task = tokio::spawn(async move {
        echo.run().await;
    });
    let _ = tokio::join!(master_task, echo_task);
    // sleep
    tokio::time::sleep(Duration::from_secs(1000)).await;
}
