use std::str;
use std::rc::Rc;
use std::cell::RefCell;
#[cfg(feature = "ssl")]
use std::thread::sleep;
#[cfg(feature = "ssl")]
use std::time::Duration;

use serde_json::Value;

use ws::{listen, Handler, Result, Message, Handshake, CloseCode};
#[cfg(feature = "ssl")]
use ws::util::TcpStream;

#[cfg(feature = "ssl")]
use std::fs::File;
#[cfg(feature = "ssl")]
use std::io::Read;

#[cfg(feature = "ssl")]
use openssl::pkey::PKey;
#[cfg(feature = "ssl")]
use openssl::ssl::{SslAcceptor, SslMethod, SslStream};
#[cfg(feature = "ssl")]
use openssl::x509::X509;

use node::Node;
use network::Network;

#[cfg(feature = "ssl")]
struct Server {
    node: Rc<RefCell<Node>>,
    ssl: Rc<SslAcceptor>,
    network: Rc<RefCell<Network>>,
}

#[cfg(not(feature = "ssl"))]
struct Server {
    node: Rc<RefCell<Node>>,
    network: Rc<RefCell<Network>>,
}

impl Server {
    #[cfg(feature = "push")]       
    fn handle_push_requests(&mut self, json_message: &Value) {  
        match json_message["action"].as_str() {
            Some("subscribe-push") => { 
                    match json_message["subscriptionData"].as_str() {
                            Some(data) => {
                                self.network.borrow_mut().add_subscription(data, &self.node);
                            },
                            _ => { println!("No subscription data") }
                    }
                },
            Some("connection-request") => {
                match json_message["endpoint"].as_str() {
                    Some(endpoint) => {
                        let user_sending_request = self.node.borrow().owner.clone().unwrap();
                        self.network.borrow().send_push(&user_sending_request, &endpoint);
                    }
                    _ => { println!("No endpoint for connection request") }
                }
            },
            _ => { /* Do nothing if the user is not interested in the push */ }
        };
    }

    fn handle_connection_request(&self, json_message: &Value, text_message: &str) -> Result<()> {
        // !!! WARNING !!!
        // The word "protocol" match is protcol specific.
        // Thus a client should make sure to send a viable protocol
        let protocol = match json_message["protocol"].as_str() {
            Some(desired_protocol) => { Some(desired_protocol) },
            _ => { None }
        };


        // The words below are protcol specific.
        // Thus a client should make sure to use a viable protocol
        match protocol {
            Some("one-to-all") => {
                self.node.borrow().sender.broadcast(text_message)
            },
            Some("one-to-self") => {
                self.node.borrow().sender.send(text_message)
            },
            Some("one-to-room") => {
                match json_message["room"].as_str() {
                    Some(room_name) => {
                        let network = self.network.borrow();
                        network.rooms.borrow().get(room_name).map(|room| { 
                            // Send the message to everyone in the room
                            for node in room.nodes.borrow().iter() {
                                node.upgrade().map(|upgraded_node| {
                                    if let Some(owner) = upgraded_node.borrow().owner.as_ref() {
                                        let from = json_message["from"].as_str();
                                        if from != Some(owner.as_str()) {
                                            upgraded_node.borrow().sender.send(text_message);
                                        }
                                    }
                                });
                            }
                        });
                        Ok(())
                    }
                    _ => {
                        self.node.borrow().sender.send(
                            "No field 'room' provided"
                        )
                    }
                }
            },
            Some("one-to-one") => {
                match json_message["endpoint"].as_str() {
                    Some(endpoint) => {
                        let network = self.network.borrow();
                        let endpoint_node = network.nodemap.borrow().get(endpoint)
                            .and_then(|node| node.upgrade());

                        match endpoint_node {
                            Some(node) => { node.borrow().sender.send(text_message) }
                            _ => {self.node.borrow().sender
                                .send("Could not find a node with that name")}
                        }
                    }
                    _ => {
                        self.node.borrow().sender.send(
                            "No field 'endpoint' provided"
                        )
                    }
                }
                
            }
            _ => {
                self.node.borrow().sender.send(
                        "Invalid protocol, valid protocols include: 
                            'one-to-self'
                            'one-to-one'
                            'one-to-room'
                            'one-to-all'"
                    )
                }
        }
    }
}

impl Handler for Server {
    fn on_open(&mut self, handshake: Handshake) -> Result<()> {
        // Get the aruments from a URL
        // i.e localhost:8000/user=testuser
        let url_arguments : Vec<&str> = handshake.request.resource()[2..]
                                .split(|c| c == '&' || c == '=').collect();
        // Beeing greedy by not collecting pairs
        // Instead every even number (including 0) will be an identifier
        // and every odd number will be the assigned value
        
        if url_arguments[0] == "user" {
            let username: &str = url_arguments[1]; // This can panic
            self.network.borrow_mut().add_user(username, &self.node);
        }


        if url_arguments[2] == "room" {
            // TODO  ADD ORIGIN
            //let origin = handshake.request.origin().unwrap().unwrap();
            let room_name = url_arguments[3]; //format!("{}/{}", origin, url_arguments[3]); // possible panic
            self.network.borrow_mut().create_room(&room_name);
            self.network.borrow_mut().add_user_to_room(&room_name, &self.node);
        }

        println!("Network expanded to {:?} connected nodes", self.network.borrow().size());
        Ok(())
    }

    #[cfg(feature = "ssl")]       
    fn upgrade_ssl_server(&mut self, sock: TcpStream) -> ws::Result<SslStream<TcpStream>> {
        println!("Server node upgraded");
        // TODO  This is weird, but the sleep is needed...
        sleep(Duration::from_millis(200));
        self.ssl.accept(sock).map_err(From::from)
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        let text_message: &str = msg.as_text()?;
        let json_message: Value = 
            serde_json::from_str(text_message).unwrap_or(Value::default());
     
        // Use chain of responsibility to handle the requests
        #[cfg(feature = "push")]
        self.handle_push_requests(&json_message);

        self.handle_connection_request(&json_message, &text_message)

    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        // Remove the node from the network
        if let Some(owner) = &self.node.borrow().owner {
            match code {
                CloseCode::Normal =>
                    println!("{:?} is done with the connection.", owner),
                CloseCode::Away =>
                    println!("{:?} left the site.", owner),
                CloseCode::Abnormal =>
                    println!("Closing handshake for {:?} failed!", owner),
                _ =>
                    println!("{:?} encountered an error: {:?}", owner, reason),
            };
        
            self.network.borrow_mut().remove(owner)
        }
        
        println!("Network shrinked to {:?} connected nodes\n", self.network.borrow().size());
    }

    fn on_error(&mut self, err: ws::Error) {
        println!("The server encountered an error: {:?}", err);
    }
}


#[cfg(feature = "ssl")]
fn read_file(name: &str) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(name)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

#[cfg(not(feature = "ssl"))]
pub fn run() {
    // Setup logging
    env_logger::init();

    // setup command line arguments
    #[cfg(not(feature = "push"))]
    let matches = clap::App::new("Rustysignal")
        .version("2.0.0")
        .author("Rasmus Viitanen <rasviitanen@gmail.com>")
        .about("A signaling server implemented in Rust that can be used for e.g. WebRTC, see https://github.com/rasviitanen/rustysignal")
        .arg(
            clap::Arg::with_name("ADDR")
                .help("Address on which to bind the server e.g. 127.0.0.1:3012")
                .required(true)
                .index(1),
        )
        .get_matches();

    #[cfg(feature = "push")]
    let matches = clap::App::new("Rustysignal")
        .version("2.0.0")
        .author("Rasmus Viitanen <rasviitanen@gmail.com>")
        .about("A signaling server implemented in Rust that can be used for e.g. WebRTC, see https://github.com/rasviitanen/rustysignal")
        .arg(
            clap::Arg::with_name("ADDR")
                .help("Address on which to bind the server e.g. 127.0.0.1:3012")
                .required(true)
                .index(1),
        )
        .arg(
            clap::Arg::with_name("VAPIDKEY")
                .help("A NIST P256 EC private key to create a VAPID signature, used for push")
                .required(true)
                .index(2),
        )
        .get_matches();
    
    println!("------------------------------------");
    println!("rustysignal is listening on address");
    println!("ws://{}", matches.value_of("ADDR").unwrap());
    println!("To use SSL you need to reinstall rustysignal using 'cargo install rustysignal --features ssl --force");
    println!("To enable push notifications, you need to reinstall rustysignal using 'cargo install rustysignal --features push --force");
    println!("For both, please reinstall using 'cargo install rustysignal --features 'ssl push' --force");
    println!("-------------------------------------");
    
    let network = Rc::new(RefCell::new(Network::default()));
    
    #[cfg(feature = "push")]
    network.borrow_mut().set_vapid_path(matches.value_of("VAPIDKEY").unwrap());    

    listen(matches.value_of("ADDR").unwrap(),
        |sender| {
            let node = Node::new(sender);
            Server { 
                node: Rc::new(RefCell::new(node)),
                network: network.clone()
            }
        }
    ).unwrap()
}

#[cfg(feature = "ssl")]
pub fn run() {
    // Setup logging
    env_logger::init();

    // setup command line arguments
    #[cfg(feature = "push")]
    let matches = clap::App::new("Rustysignal")
        .version("2.0.0")
        .author("Rasmus Viitanen <rasviitanen@gmail.com>")
        .about("A secure signaling server implemented in Rust that can be used for e.g. WebRTC, see https://github.com/rasviitanen/rustysignal")
        .arg(
            clap::Arg::with_name("ADDR")
                .help("Address on which to bind the server.")
                .required(true)
                .index(1),
        )
        .arg(
            clap::Arg::with_name("CERT")
                .help("Path to the SSL certificate.")
                .required(true)
                .index(2),
        )
        .arg(
            clap::Arg::with_name("KEY")
                .help("Path to the SSL certificate key.")
                .required(true)
                .index(3),
        ).arg(
            clap::Arg::with_name("VAPIDKEY")
                .help("Path to NIST P256 EC private key to create a VAPID signature, used for push")
                .required(true)
                .index(4),
        )
        .get_matches();

    #[cfg(not(feature = "push"))]
    let matches = clap::App::new("Rustysignal")
        .version("2.0.0")
        .author("Rasmus Viitanen <rasviitanen@gmail.com>")
        .about("A secure signaling server implemented in Rust that can be used for e.g. WebRTC, see https://github.com/rasviitanen/rustysignal")
        .arg(
            clap::Arg::with_name("ADDR")
                .help("Address on which to bind the server.")
                .required(true)
                .index(1),
        )
        .arg(
            clap::Arg::with_name("CERT")
                .help("Path to the SSL certificate.")
                .required(true)
                .index(2),
        )
        .arg(
            clap::Arg::with_name("KEY")
                .help("Path to the SSL certificate key.")
                .required(true)
                .index(3),
        )
        .get_matches();
    
    let cert = {
        let data = read_file(matches.value_of("CERT").unwrap()).unwrap();
        X509::from_pem(data.as_ref()).unwrap()
    };

    let pkey = {
        let data = read_file(matches.value_of("KEY").unwrap()).unwrap();
        PKey::private_key_from_pem(data.as_ref()).unwrap()
    };

    let acceptor = Rc::new({
        println!("Building acceptor");
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        builder.set_private_key(&pkey).unwrap();
        builder.set_certificate(&cert).unwrap();

        builder.build()
    });

    println!("------------------------------------");
    println!("rustysignal is listening on securily on address");
    println!("wss://{}", matches.value_of("ADDR").unwrap());
    println!("To disable SSL you need to reinstall rustysignal using 'cargo install rustysignal --force");
    println!("To enable push notifications, you need to reinstall rustysignal using 'cargo install rustysignal --features 'ssl push' --force");
    println!("-------------------------------------");
    
    let network = Rc::new(RefCell::new(Network::default()));

    #[cfg(feature = "push")]
    network.borrow_mut().set_vapid_path(matches.value_of("VAPIDKEY").unwrap());

    ws::Builder::new()
        .with_settings(ws::Settings {
            encrypt_server: true,
            ..ws::Settings::default()
        })
        .build(|sender: ws::Sender| {
            println!("Building server");
            let node = Node::new(sender);
            Server {
                node: Rc::new(RefCell::new(node)),
                ssl: acceptor.clone(),
                network: network.clone()
            }
        })
        .unwrap().listen(matches.value_of("ADDR").unwrap())
    .unwrap();
}