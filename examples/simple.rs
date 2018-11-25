
extern crate yamux;
extern crate tokio;
extern crate futures;
extern crate env_logger;
extern crate log;

use std::time::Duration;
use std::thread;

use yamux::{
    config::Config,
    session::Session,
    stream::StreamHandle,
};
use futures::Stream;
use futures::Future;
use tokio::io::{copy, AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use log::{error, warn, info, debug, trace};

fn main() {
    env_logger::init();
    info!("Starting ......");
    thread::spawn(run_server);
    thread::sleep(Duration::from_secs(1));
    run_client();
}

fn run_server() {
   // Bind the server's socket.
    let addr = "127.0.0.1:12345".parse().unwrap();
    let listener = TcpListener::bind(&addr)
        .expect("unable to bind TCP listener");

    // Pull out a stream of sockets for incoming connections
    let server = listener.incoming()
        .map_err(|e| eprintln!("accept failed = {:?}", e))
        .for_each(|sock| {
            info!("accepted a socket: {:?}", sock.peer_addr());
            let session = Session::new_server(sock, Config::default());
            // Split up the reading and writing parts of the
            // socket.
            let fut = session
                .for_each(|stream| {
                    info!("Server accept a stream from client: id={}", stream.id());
                    Ok(())
                })
                .map_err(|err| {
                    println!("server stream error: {:}", err);
                    ()
                });

            // Spawn the future as a concurrent task.
            tokio::spawn(fut)
        });

    // Start the Tokio runtime
    tokio::run(server);
}

fn run_client() {
    use tokio::net::TcpStream;
    let addr = "127.0.0.1:12345".parse().unwrap();
    let socket = TcpStream::connect(&addr)
        .and_then(|sock| {
            info!("connected to server: {:?}", sock.peer_addr());
            let mut session = Session::new_client(sock, Config::default());

            match session.open_stream() {
                Ok(stream) => {
                    info!("success open stream from client: id={}", stream.id());
                }
                Err(err) => {
                    error!("client open stream error: {:?}", err);
                }
            }
            session
                .for_each(|stream| {
                    info!("client accept a stream from server: id={}", stream.id());
                    Ok(())
                })
        })
        .map_err(|err| {
            println!("client error: {:?}", err);
            ()
        });
    tokio::run(socket);
}
