extern crate futures;
extern crate hyper;
extern crate tokio;

use std::fs::File;
use std::path::Path;
use std::io::prelude::*;

use futures::future;

use hyper::rt::{Future, Stream};
use hyper::service::Service;
use hyper::{Body, Method, Request, Response, Server, StatusCode};

use tokio::executor::spawn;

/// We need to return different futures depending on the route matched,
/// and we can do that with an enum, such as `futures::Either`, or with
/// trait objects.
///
/// A boxed Future (trait object) is used as it is easier to understand
/// and extend with more types. Advanced users could switch to `Either`.
type BoxFut = Box<Future<Item = Response<Body>, Error = hyper::Error> + Send>;

struct Proxy;

#[derive(Debug)]
enum Error {
    FSError(std::io::Error),
    HyperError(hyper::Error)
}

impl Service for Proxy {
    type ReqBody = Body;
    type ResBody = Body;
    type Error = hyper::Error;
    type Future = BoxFut;

    fn call(&mut self, req: Request<Body>) -> BoxFut {
        let (parts, body) = req.into_parts();

        match (parts.method, parts.uri.path()) {
            (Method::GET, "/") => {
                let mut response = Response::new(Body::empty());

                *response.body_mut() = Body::from("Try POSTing data to /echo");
                Box::new(future::ok(response))
            }

            (Method::POST, _) => {
                
                let uri = parts.uri.clone();
                
                let (sender, receiver) = futures::sync::oneshot::channel::<Result<(), Error>>();

                spawn(
                    future::lazy(move || {
                        let p = Path::new(uri.path());
                        let mut f = File::create(p.components().last().unwrap().as_os_str()).unwrap();

                        body.map_err(Error::HyperError)
                        .for_each(move |chunk| {
                            f.write(chunk.as_ref())
                                .map(|_| () )
                                .map_err(Error::FSError)
                        })
                    })
                    .then(|result| {
                        sender.send(result)
                    })
                    .map_err(|e| panic!("Unrecoverable error: couldn't send data back to handler: {:?}", e))
                );


                let f = receiver.then(|r| {
                    let mut response = Response::new(Body::empty());

                    match r {
                        Ok(_) => {}
                        Err(e) => {
                            *response.body_mut() = Body::from(format!("An error occured: {:?}", e));
                            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                        }
                    }
                    future::ok(response)
                });
                
                Box::new(f)
            }

            // The 404 Not Found route...
            _ => {
                let mut response = Response::new(Body::empty());

                *response.status_mut() = StatusCode::NOT_FOUND;
                Box::new(future::ok(response))
            }
        }

    }
}

fn spawn_service() -> Result<Proxy, hyper::Error> {
    Ok(Proxy)
}

fn main() {
    let addr = ([127, 0, 0, 1], 3000).into();

    let server = Server::bind(&addr)
        .serve(|| spawn_service() )
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Listening on http://{}", addr);
    hyper::rt::run(server);
}
