
use natord::compare;
use std::cell::{RefCell};
use std::fmt::Debug;

use std::rc::{Rc, Weak};


use crate::Pack::PackFile;
use crate::Settings::Pathlib;

pub struct tree_node<T> {
    pub value: T,
    pub parent: RefCell<Weak<tree_node<T>>>,
    pub children: RefCell<Vec<Rc<tree_node<T>>>>,
    pub path: Pathlib
}

impl<T> tree_node<T>
where
    T: Debug,
    T: PartialEq,
    T: Ord
{
    pub fn new(value: T, full_path: String) -> Rc<tree_node<T>> {
        Rc::new(tree_node {
            value: value,
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(vec![]),
            path: Pathlib::new(full_path)
        })
    }

    fn add_child(parent: &Rc<tree_node<T>>, child: &Rc<tree_node<T>>) {
        child.parent.replace(Rc::downgrade(parent));
        //or *child.parent.borrow_mut() = Rc::downgrade(parent);
        parent.children.borrow_mut().push(Rc::clone(child));
    }

    pub fn is_root(&self) -> bool {
        self.parent.borrow().upgrade().is_none()
    }

    pub fn is_leaf(&self) -> bool {
        self.children.borrow().is_empty()
    }

    pub fn print(node: &Rc<tree_node<T>>, indent: usize) {
        println!("{}{:?}", "| ".repeat(indent), node.value);
        for child in node.children.borrow().iter() {
            tree_node::print(child, indent + 1)
        }
    }

    fn find_by_value(node: &Rc<Self>, value: &T) -> Option<Rc<Self>> {
        // Find a node with given value, assuming all nodes have unique values
        if node.value == *value {
            return Some(Rc::clone(node));
        }

        for child in node.children.borrow().iter() {
            if let Some(found) = Self::find_by_value(child, value) {
                return Some(found);
            }
        }
        None
    }

    fn is_value_in_children(&self, value: &T) -> Option<Rc<Self>> {
        // check if any children node has value
        for child in self.children.borrow().iter() {
            if child.value == *value {
                return Some(Rc::clone(child));
            }
        }
        return None;
    }

    fn sort(&self) {
        self.children.borrow_mut().sort_by(|a, b|{
            a.value.cmp(&b.value)
        });
        for child in self.children.borrow().iter() {
            child.sort();
        }
    }
}


/* 
fn get_parent_node_if_exists(
    root_node: &Rc<tree_node<String>>,
    value: String,
) -> Rc<tree_node<String>> {
    match tree_node::find_by_value(root_node, &value.clone()) {
        Some(node) => {
            return node;
        }
        None => {
            let new_node = tree_node::new(value);
            tree_node::add_child(&root_node, &new_node);
            return new_node;
        }
    }
}*/

pub fn update_from_sarc_paths(root_node: &Rc<tree_node<String>>, sarc_file: &PackFile) {
    let mut  paths: Vec<String> = Default::default();
    for file in sarc_file.sarc.files() {
        match file.name() {
            Some(f) => {paths.push(f.to_string())},
            None => {}
        }
    }
    
    //paths.sort(); doesnt sort like windows
    paths.sort_by(|a, b| compare(&a, &b));
    update_from_paths(&root_node, paths);
}

pub fn update_from_paths(node: &Rc<tree_node<String>>, paths: Vec<String>) {
    for path in paths {
        let elements: Vec<&str> = path.split("/").collect();
        update_root_from_list(&node, elements, path.clone());
    }
}

fn update_root_from_list(root_node: &Rc<tree_node<String>>, elements: Vec<&str>, path: String) {
    let mut cur_node: Rc<tree_node<String>> = Rc::clone(root_node);
    let size: usize = elements.len();
    for (i, elem) in elements.iter().enumerate() {
        if let Some(node) = tree_node::is_value_in_children(&cur_node, &elem.to_string()) {
            cur_node = Rc::clone(&node);
        } else {
            let mut cur_path = elements[0].to_string(); //root immediate children
            if i > 0 {
                if i < size -1 {
                    for k in 1..i+1 {
                        cur_path = format!("{}/{}", cur_path, elements[k].to_string());
                    }
                } else { //leaf
                    cur_path = path.clone();
                }
            }
            println!("Added node name {} full path {}", &elem, &cur_path);
            let child = tree_node::new(elem.to_string(), cur_path);
            tree_node::add_child(&cur_node, &child);
            cur_node = Rc::clone(&child);
        }
    }
}

/* 
pub fn test_paths_tree() {
    let mut paths: Vec<String> = vec![
        "coding/TotkBits/src/asdf.txt".to_string(),
        "coding/zxcv.txt".to_string(),
        "coding/TotkBits/qwer.txt".to_string(),
    ];
    let root_node = tree_node::new("Root".to_string());
    update_from_paths(&root_node, paths);
    tree_node::print(&root_node, 0);
}
pub fn test_tree() -> Rc<tree_node<String>> {
    let root = tree_node::new("Root".to_string());
    let child1 = tree_node::new("Child1".to_string());
    let child2 = tree_node::new("Child2".to_string());
    let child1_1 = tree_node::new("Child1_1".to_string());

    tree_node::add_child(&root, &child1);
    tree_node::add_child(&root, &child2);
    tree_node::add_child(&child1, &child1_1);
    //tree_node::print(&root, 0);
    //println!("{:?} {:?}", tree_node::is_root(&root), tree_node::is_leaf(&child2));
    root
}
*/
