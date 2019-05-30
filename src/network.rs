//! A network to keep track of every node connected to the signaling server.
//! The signaling server is able to compile to either a network with 
//! push functionality or not. In  the case of push, the network will include a push map.
//! The push map includes subscriptions, that includes information to discover 
//! user's browser endpoints.

use std::str;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
#[cfg(feature = "push")]
use std::{
    fs::File,
    time::Duration,
};

#[cfg(feature = "push")]
use futures::{
    future::{
        lazy,
    },
    Future,
};
#[cfg(feature = "push")]
use web_push::*;

use node::Node;
use room::Room;

/// A network for keeping track of the connected nodes and the pushmap.
/// The weak pointer to the nodes will allow nodes to disconnect, and 
/// automatically invalidate the node, yet keep the username bound to the network
/// 
/// The reference counting is for the network is necessary, since the network should die if no one is connected.
/// We do not want to replicate multiple networks. This assures memory efficiency.
/// 
/// The vapit path is some system path to a file containing the vapid private key, 
/// and is necessary to get permission to send push notifications
/// 
/// The owneship of the push information is stored here instad of in the nodes,
/// since we want to be able to send push notifications to disconnected nodes.
#[cfg(feature = "push")]
#[derive(Default)]
pub struct Network {
    pub nodemap: Rc<RefCell<HashMap<String, Weak<RefCell<Node>>>>>,
    pub pushmap: Rc<RefCell<HashMap<String, String>>>,
    pub rooms: Rc<RefCell<HashSet<Room>>>,

    pub vapid_path: String,
}

/// A network for keeping track of the connected nodes.
/// The weak pointer to the nodes will allow nodes to disconnect, and 
/// automatically invalidate the node, yet keep the username bound to the network.
/// 
/// The reference counting is for the network is necessary,, since the network should die if no one is connected.
/// And we do not want to replicate multiple networks. This assures memory efficiency.
#[cfg(not(feature = "push"))]
#[derive(Default)]
pub struct Network {
    pub nodemap: Rc<RefCell<HashMap<String, Weak<RefCell<Node>>>>>,
    pub rooms: Rc<RefCell<HashSet<Room>>>,
}

impl Network {
    /// Adds a user to the network, making sure to not override current usernames on the network
    #[inline]
    pub fn add_user(&mut self, owner: &str, node: &std::rc::Rc<std::cell::RefCell<Node>>) {
        if !self.nodemap.borrow().contains_key(owner) {
            node.borrow_mut().owner = Some(owner.into());
            self.nodemap.borrow_mut().insert(owner.to_string(), Rc::downgrade(node));
            println!("Node {:?} connected to the network.", owner);
        } else {
            println!("{:?} tried to connect, but the username was taken", owner);
            node.borrow().sender.send("The username is taken").ok();
        }
    }

    pub fn create_room(&mut self, room_name: &str) {
        if self.rooms.borrow_mut().insert(Room::new(room_name)) {
            println!("Created new room {:?}", room_name);
        };
    }

    #[inline]
    pub fn add_user_to_room(&mut self, room_name: &str, node: &std::rc::Rc<std::cell::RefCell<Node>>) {
        self.rooms.borrow_mut().get(room_name).and_then(|room| 
            Some(room.nodes.borrow_mut().push(Rc::downgrade(node))));       
    }

    /// Removes a user from the network, typically when the connection is ended.
    #[inline]    
    pub fn remove(&mut self, owner: &str) {
        self.nodemap.borrow_mut().remove(owner);
    }

    /// Retrieves the number of connected nodes on the network, useful for balance loading.
    #[inline]
    pub fn size(&self) -> usize {
        self.nodemap.borrow().len()
    }
    
    /// Adds a subscription, that enables the node's browser endpoint to be discovered.
    /// This makes it possible to send push notifications to those subscriptions.
    #[cfg(feature = "push")]
    pub fn add_subscription(&mut self, subscription: &str, node: &std::rc::Rc<std::cell::RefCell<Node>>) {
        println!("Node {:?} updated its subscription data", node.borrow().owner);
        node.borrow_mut().subscription = Some(subscription.into());
        let owner = node.borrow().owner.clone();
        
        self.pushmap.borrow_mut().insert(owner.unwrap(), subscription.to_string());
    }

    /// Sets the system path to a vapid private key used for push
    #[cfg(feature = "push")]
    pub fn set_vapid_path(&mut self, vapid_path: &str) {
        self.vapid_path = vapid_path.to_string();
    }

    /// Sends a push to an endpoint. The endpoint subscription is discovered by 
    /// looking it up in the network's push map.
    #[cfg(feature = "push")]
    pub fn send_push(&self, sender: &str, endpoint: &str) {
        println!("!!!!!! Sending PUSH !!!!!!!");

        let payload = 
            json!({"body": format!("{}\nwants to connect with you", sender), 
            "sender": sender, 
            "actions": [
                {"action": "allowConnection", "title": "✔️ Allow"}, 
                {"action": "denyConnection", "title": "✖️ Deny"}]}).to_string();

        if let Some(subscription) = self.pushmap.borrow().get(endpoint) {
            let subscription_info: SubscriptionInfo = serde_json::from_str(subscription).unwrap();

            let mut builder = WebPushMessageBuilder::new(&subscription_info).unwrap();
            builder.set_payload(ContentEncoding::AesGcm, payload.as_bytes());

            let vapid_file = File::open(&self.vapid_path).unwrap();

            let sig_builder = VapidSignatureBuilder::from_pem(vapid_file, &subscription_info).unwrap();
            let signature = sig_builder.build().unwrap();

            builder.set_ttl(3600);
            builder.set_vapid_signature(signature);

            match builder.build() {
                Ok(message) => {
                    let client = WebPushClient::new().unwrap();
                    tokio::run(lazy(move || {
                        client
                            .send_with_timeout(message, Duration::from_secs(4))
                            .map(|response| {
                                println!("Sent: {:?}", response);
                            }).map_err(|error| {
                                println!("Error: {:?}", error)
                            })
                    }));
                },
                Err(error) => {
                    println!("ERROR in building message: {:?}", error)
                }
            }
        }
    }
}