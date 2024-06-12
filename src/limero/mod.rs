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

pub struct ActorRef<T> {
    sender: Sender<T>,
    receivers : Vec<Receiver<T>>,
}

impl<T> ActorRef<T> {
    pub fn new(sender: Sender<T>) -> Self {
        ActorRef { sender, receivers: Vec::new()}
    }
    pub fn tell(&self, m: T) {
        let _ = self.sender.try_send(m);
    }
}

pub struct Emitter<T> {
    listeners: Vec<ActorRef<T>>,
}

impl<T> Emitter<T> {
    pub fn new() -> Self {
        Emitter { listeners: Vec::new() }
    }
    pub fn add_listener(&mut self, sender: ActorRef<T>) {
        self.listeners.push(sender);
    }
    pub fn emit(&self, m: T) where T:Clone {
        for sender in self.listeners.iter() {
            sender.tell(m.clone());
        }
    }
}

pub trait ActorTrait<T,U> {
    async fn run();
    fn actor_ref(&self) -> ActorRef<T>;   
    fn add_listener(&self, listener: ActorRef<U>);
}

trait HasSink<M> {
    fn sink(&self) -> Box<dyn SinkTrait<M>>;
}

pub trait SourceTrait<T> {
    fn emit(&self, m: &T);
    fn add_sink(&self, sink: Box<dyn SinkTrait<T>>);
}

pub struct Source<M> {
    senders: Vec<Sender<M>>,
    sinks: Arc<RwLock<Vec<Box<dyn SinkTrait<M>>>>>,
}

impl<M> Source<M>
where
    M: Clone,
{
    pub fn new() -> Self {
        Source {
            senders: Vec::new(),
            sinks: Arc::new(RwLock::new(Vec::new())),
        }
    }
    pub fn add_sink(&self, sink: Box<dyn SinkTrait<M>>) {
        self.sinks.write().unwrap().push(sink);
    }
    pub fn emit(&self, m: &M) {
        for sender in self.senders.iter() {
            if sender.try_send(m.clone()).is_err() {
                error!(" could not send data ");
            };
        }
        for sink in self.sinks.read().unwrap().iter() {
            sink.push(m.clone());
        }
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
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        Sink { tx, rx }
    }
    pub async fn read(&mut self) -> Option<M> {
        self.rx.recv().await
    }
    fn sender(&self) -> Sender<M> {
        self.tx.clone()
    }
    pub fn sink(&self) -> Box<dyn SinkTrait<M>>
    where
        M: Clone + Send + Sync,
    {
        struct Sinker<N> {
            tx: Arc<Sender<N>>,
        }
        impl<N> SinkTrait<N> for Sinker<N>
        where
            N: Clone + Send + Sync,
        {
            fn push(&self, m: N) {
                let _ = self.tx.try_send(m).is_err_and(|err| {
                    error!(" could not send data {:?}", err);
                    false
                });
            }
        }
        Box::new(Sinker::<M> {
            tx: Arc::new(self.tx.clone()),
        })
    }
}

/*impl<T> HasSink<T> for Sink<T> where T:Clone+Send+Sync+'static{
    fn sink(&self) -> Box<dyn SinkTrait<T>> {
        struct Sinker<N> {
            tx:Arc<Sender<N>>
        }
        impl<N> SinkTrait<N> for Sinker<N> where N:Clone+Send+Sync  {
            fn push(&self,m:N) {
                let _r = self.tx.try_send(m);
            }
        }
        Box::new( Sinker::<T> { tx : Arc::new(self.tx.clone()) })
    }
}*/

impl<M> SinkTrait<M> for Sink<M>
where
    M: Clone + Send + Sync,
{
    fn push(&self, m: M) {
        let _r = self.tx.try_send(m);
    }
}

impl<M> Shr<Box<dyn SinkTrait<M>>> for &Source<M>
where
    M: Clone + Send + Sync,
{
    type Output = ();
    fn shr(self, rhs: Box<dyn SinkTrait<M>>) -> Self::Output {
        self.add_sink(rhs);
    }
}

impl<M> Shr<&Sink<M>> for &Source<M>
where
    M: Clone + Send + Sync + 'static,
{
    type Output = ();
    fn shr(self, rhs: &Sink<M>) -> Self::Output
    where
        M: Clone + Send + Sync,
    {
        self >> rhs.sink();
    }
}
// =====================================  FuncFlow =====================================
struct FuncFlow<T, U, F>
where
    F: Fn(T) -> Option<U> + Send + Sync,
    T: Clone + Send + Sync,
    U: Clone + Send + Sync,
{
    f: F,
    sinks: Arc<RwLock<Vec<Box<dyn SinkTrait<U>>>>>,
    t: PhantomData<T>,
}

impl<T, U, F> FuncFlow<T, U, F>
where
    F: Fn(T) -> Option<U> + Send + Sync,
    T: Clone + Send + Sync,
    U: Clone + Send + Sync,
{
    fn new(f: F) -> Self {
        FuncFlow::<T, U, F> {
            f,
            sinks: Arc::new(RwLock::new(Vec::new())),
            t: PhantomData,
        }
    }
}

impl<T, U, F> SinkTrait<T> for FuncFlow<T, U, F>
where
    F: Fn(T) -> Option<U> + Send + Sync,
    T: Clone + Send + Sync,
    U: Clone + Send + Sync,
{
    fn push(&self, t: T)
    where
        T: Clone + Send + Sync,
        U: Clone + Send + Sync,
    {
        if let Some(u) = (self.f)(t) {
            self.emit(&u.clone());
        }
    }
}

impl<T, U, F> SourceTrait<U> for FuncFlow<T, U, F>
where
    F: Fn(T) -> Option<U> + Send + Sync,
    T: Clone + Send + Sync,
    U: Clone + Send + Sync,
{
    fn emit(&self, t: &U) {
        for sink in self.sinks.read().unwrap().iter() {
            sink.push(t.clone());
        }
    }
    fn add_sink(&self, sink: Box<dyn SinkTrait<U>>) {
        self.sinks.write().unwrap().push(sink);
    }
}
//====================================== Shr =====================================
impl<T, U, F> Shr<F> for Source<T>
where
    F: Fn(T) -> Option<U> + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    type Output = Box<dyn SourceTrait<U>>;

    fn shr(self, f: F) -> Box<dyn SourceTrait<U>> {
        let ff = Box::new(FuncFlow::<T, U, F>::new(f));
        // self.add_sink( ff  );
        ff
    }
}

impl<T> Shr<Box<dyn SinkTrait<T>>> for Box<dyn SourceTrait<T>>
where
    T: Clone + Send + Sync + 'static,
{
    type Output = ();
    fn shr(self, rhs: Box<dyn SinkTrait<T>>) -> Self::Output {
        self.add_sink(rhs);
    }
}

struct PreProcessor<T, U> {
    f: Box<dyn Fn(T) -> Option<U> + Send + Sync>,
    sink: Box<dyn SinkTrait<U>>,
    t: PhantomData<T>,
}

impl<T, U> PreProcessor<T, U> {
    fn new(f: Box<dyn Fn(T) -> Option<U> + Send + Sync>, sink: Box<dyn SinkTrait<U>>) -> Self {
        PreProcessor::<T, U> {
            f,
            sink,
            t: PhantomData,
        }
    }
}

impl<T, U> SinkTrait<T> for PreProcessor<T, U>
where
    T: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    fn push(&self, t: T) {
        if let Some(u) = (self.f)(t) {
            self.sink.push(u);
        }
    }
}

pub fn pre_process<T, U, F>(f: F, sink: Box<dyn SinkTrait<U>>) -> Box<dyn SinkTrait<T>>
where
    F: Fn(T) -> Option<U> + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    let pp = PreProcessor::<T, U>::new(Box::new(f), sink);
    Box::new(pp)
}
