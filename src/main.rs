#![allow(unused_imports)]
#![allow(dead_code)]
#[cfg(all(feature = "std", feature = "no_std"))]
compile_error!("feature \"std\" and feature \"no_std\" cannot be enabled at the same time");

#[cfg(feature = "tokio")]
use {
    std::collections::BTreeMap,
    std::vec::Vec,
    std::io::Write,
    std::pin::pin,
    std::rc::Rc,
    std::sync::Arc,
    std::thread::sleep,
    std::time::{Duration, Instant},
    std::{ops::Shr, pin::Pin},
    std::borrow::BorrowMut,
    std::cell::RefCell,
};

use minicbor::decode::info;
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::Receiver;    

use std::sync::Mutex as StdMutex;
use core::result::Result;
use std::sync::RwLock;
mod logger;
use log::{debug, info};
use std::error::Error;


trait SinkTrait<M> : Send + Sync{
    fn push(&self,m:M);
}

trait HasSink<M> {
    fn sink(&self) -> Box<dyn SinkTrait<M>>;
}



struct Source<M> {
    senders : Vec<Sender<M>>,
    sinks : Arc<RwLock<Vec<Box<dyn SinkTrait<M>>>>>,
}

impl<M> Source<M> where M : Clone{
    fn new() -> Self {
        Source {
            senders : Vec::new(),
            sinks : Arc::new(RwLock::new(Vec::new())),
        }
    }
    fn add_sender(&mut self,sender:Sender<M>) {
       // self.senders.push(sender);
    }
    fn add_sink(&self,sink:Box<dyn SinkTrait<M>>) {
        self.sinks.write().unwrap().push(sink);
    }
    fn emit(&self,m:&M) {
        for sender in self.senders.iter() {
            let _r = sender.try_send(m.clone());
        }
        for sink in self.sinks.read().unwrap().iter() {
            sink.push(m.clone());
        }
    }
}

struct Sink<M> {
    rx : Receiver<M>,
    tx : Sender<M>,
}


impl<M> Sink<M> where M:Clone + Send + Sync +'static{
    fn new() -> Self {
        let ( tx,rx) = tokio::sync::mpsc::channel(100);
        Sink {
            tx, rx,
        }
    }
    async fn read(&mut self) -> Option<M> {
        self.rx.recv().await
    }
    fn sender(&self) -> Sender<M> {
        self.tx.clone()
    }
    fn sink(&self) -> Box<dyn SinkTrait<M>> where M:Clone+Send+Sync{
        struct Sinker<N> {
            tx:Arc<Sender<N>>
        }
        impl<N> SinkTrait<N> for Sinker<N> where N:Clone+Send+Sync  {
            fn push(&self,m:N) {
                let _r = self.tx.try_send(m);
            }
        }
        Box::new( Sinker::<M> { tx : Arc::new(self.tx.clone()) })
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

impl <M> SinkTrait<M> for Sink<M> where M:Clone+Send+Sync {
    fn push(&self,m:M) {
        let _r = self.tx.try_send(m);
    }
}

impl<M> Shr<Box<dyn SinkTrait<M>>> for & Source<M> where M:Clone+Send+Sync{
    type Output = ();
    fn shr(self, rhs: Box<dyn SinkTrait<M>>) -> Self::Output {
        self.add_sink(rhs);
    }
}

impl<M> Shr<&Sink<M>> for & Source<M> where M:Clone+Send+Sync+'static {
    type Output = ();
    fn shr(self, rhs: &Sink<M>) -> Self::Output where M:Clone+Send+Sync {
        self>>rhs.sink();
    }
}

/*impl<T, U, F> Shr<F> for Source<T>
    where F: FnOnce(T) -> Option<U>
{
    type Output = Source<U>;

    fn shr(self, f: F) -> Source<U> {
        struct MapSink<T, U, F> {
            f: F,
            sinks : Arc<RwLock<Vec<Box<dyn SinkTrait<U>>>>>,
        }
        impl <T,U,F> SinkTrait<T> for MapSink<T,U,F> where F: FnOnce(T) -> Option<U> {
            fn push(&self,m:T) {
                if let Some(m) = (self.f)(m) {
                    for sink in self.sinks.read().unwrap().iter() {
                        sink.push(m.clone());
                    }
                }
            }
        }
        let f = f;
        let map_sink = MapSink {
            f,
            sinks : self.sinks.clone(),
        };
        self.add_sink(Box::new(map_sink));
        Source {
            senders : Vec::new(),
            sinks : self.sinks.clone(),
        }
        

    }
}*/



struct Master {
    pub sink : Sink<Msg>,
    pub source : Source<Msg>,
}



impl Master {
    pub fn new() -> Self {
        Master {
            sink : Sink::new(),
            source : Source::new(),
        }
    }
    async fn run (&mut self) {
        info!("Master started"  );
        loop {
            let x = self.sink.read().await;
            if let Some(mut x) = x {
                x.i32 += 1;
                self.source.emit(&x);  
                if x.i32 % 100000 == 0 {
                    info!("Master sent : {:?}",x);
                }
            } else  {
                info!("Master received None"  );
            }
        }
    }
}

#[derive(Clone,Debug)]
struct Msg {
    pub i32 : i32,
    pub i64 : i64,
    pub f32 : f32,
    pub s:String,
}

struct Echo {
    pub sink : Sink<Msg>,
    pub source : Source<Msg>,
}
 
impl Echo {
    pub fn new() -> Self {
        Echo {
            sink : Sink::new(),
            source : Source::new(),
        }
    }
    async fn run (&mut self) {
        info!("Echo started"  );

        loop {
            let x = self.sink.read().await;
            if let Some(x) = x {
                self.source.emit(&x);
            } else {
                info!("Echo received None"  );
            }
        }
    }
}



#[tokio::main(worker_threads = 1)]
async fn main()  {
    logger::init();

    let mut master = Master::new();
    let mut echo = Echo::new(); 
    &master.source >> &echo.sink;
    &echo.source >> &master.sink;
    master.source.emit(& Msg {
        i32 : 0,
        i64 : 0,
        f32 : 0.0,
        s : "Hello".to_string(),
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
