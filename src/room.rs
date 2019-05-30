use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;
use std::cell::Cell;
use node::Node;

use std::hash::{Hash, Hasher};

#[derive(Default)]
pub struct Room {
    pub name: String,
    pub nodes: Rc<RefCell<Vec<Weak<RefCell<Node>>>>>,
}

impl Room {
    pub fn new(name: &str) -> Room {
        Room {
            name: name.to_string(),
            nodes: Rc::new(RefCell::new(Vec::new()))
        }
    } 

    pub fn add_node(&mut self, node: &std::rc::Rc<std::cell::RefCell<Node>>) {
        self.nodes.borrow_mut().push(Rc::downgrade(node));
    }

    pub fn print_nodes(&self) {
       for node in self.nodes.borrow().iter() {
            match node.upgrade() {
                Some(node) => println!("{:?}", node.borrow().owner),
                _ => println!("No node found")
            };
       }
    }
}

impl Hash for Room {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Room {
    fn eq(&self, other: &Room) -> bool {
        self.name == other.name
    }
}

impl std::borrow::Borrow<str> for Room {
    fn borrow(&self) -> &str {
       &self.name
    }
}


impl Eq for Room {}