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
use log::error;
use log::{debug, info};
use std::error::Error;


pub trait SinkTrait<M>: Send + Sync {
    fn push(&self, m: M);
}

pub trait SourceTrait<M>: Send + Sync {
     fn add_listener(&mut self, sink: &dyn SinkTrait<M>) ;
}
pub trait Flow<T,U> : SinkTrait<T> + SourceTrait<U>  where T:Clone+Send+Sync,U:Clone+Send+Sync {
    fn push(&self,t:T) ;
    fn add_listener(&mut self, sink: &dyn SinkTrait<U>) ;
}

pub struct FlowImpl<T,U> {
    sink : Rc<RefCell<Vec<T>>>,
    source: Vec<Box<dyn SinkTrait<U>>>,
}

impl<T,U> Flow<T,U> for FlowImpl<T,U> where T:Clone+Send+Sync,U:Clone+Send+Sync {
    fn push(&self,t:T) {
        
    }
    fn add_listener(&mut self, sender: &dyn SinkTrait<U>) {
    }
}



pub struct Sink<M> {
    rx: Receiver<M>,
    tx: Sender<M>,
}

impl<M> Sink<M>
where
    M: Clone + Send + Sync + 'static,
{
    pub fn new(size:usize) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(size);
        Sink { tx, rx }
    }
    pub async fn read(&mut self) -> Option<M> {
        self.rx.recv().await
    }
    pub fn sink_ref(&self) -> SinkRef<M> {
        SinkRef::new(self.tx.clone())
    }
}


impl<M> SinkTrait<M> for Sink<M>
where
    M: Clone + Send + Sync,
{
    fn push(&self, m: M) {
        let _r = self.tx.try_send(m);
    }
}

#[derive(Clone)]
pub struct SinkRef<M> {
    sender: Sender<M>,
}

impl<M> SinkRef<M> {
    fn new(sender:Sender<M>) -> Self {
        SinkRef { sender }
    }
}

impl<M> SinkTrait<M> for SinkRef<M> where M:Clone+Send+Sync{
    fn push(&self, message: M) {
        self.sender.try_send(message).unwrap();
    }
}


pub struct Source<T> {
    sinks: Vec<SinkRef<T>>,
}

impl<T> Source<T> {
    pub fn new() -> Self {
        Source { sinks: Vec::new() }
    }
    pub fn add_listener(&mut self, sender: SinkRef<T>) {
        self.sinks.push(sender);
    }
    pub fn emit(&self, m: T) where T:Clone+Send+Sync {
        for sink in self.sinks.iter() {
            sink.push(m.clone());
        }
    }
}

#[derive( Clone)]
enum PubSubReq {
    Subscribe { topic : String , dst : Option<SinkRef<PubSubEvent>> },
    Unsubscribe { dst : Option<SinkRef<PubSubEvent>> },
    Publish { event : PubSubEvent },
}

enum PubSubResp {
    Subscribed,
    Unsubscribed,
    Published,
    Failed,
}
#[derive(Clone)]
enum LinkEvent {
    Connected,
    Disconnected,
    Recv { data : Vec<u8> }
}
#[derive(Clone)]
enum LinkReq {
    Connect,
    Disconnect,
    Send { data : Vec<u8> }
}
#[derive(Clone)]
enum PubSubEvent {
    Connected ,
    Disconnected ,
    Publish { topic : String, data : Vec<u8> }
}

struct PubSub {
    sink : Sink<PubSubReq>,
    sink_link_events : Sink<LinkEvent>,
    events : Source<PubSubEvent>,
}


/* 
impl<M> Shr<SinkRef<M>> for &Source<M>
where
    M: Clone + Send + Sync,
{
    type Output = ();
    fn shr(self, rhs: SinkRef<M>) -> Self::Output {
        self.add_listener(rhs);
    }
}*/


// =====================================  FuncFlow =====================================
