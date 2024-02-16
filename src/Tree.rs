
use natord::compare;
use std::cell::{RefCell};
use std::fmt::Debug;

use std::rc::{Rc, Weak};


use crate::Pack::PackFile;
use crate::Settings::Pathlib;

pub struct TreeNode<T> {
    pub value: T,
    pub parent: RefCell<Weak<TreeNode<T>>>,
    pub children: RefCell<Vec<Rc<TreeNode<T>>>>,
    pub path: Pathlib
}

impl<T> TreeNode<T>
where
    T: Debug + PartialEq + Ord
{
    pub fn new(value: T, full_path: String) -> Rc<TreeNode<T>> {
        Rc::new(TreeNode {
            value: value,
            parent: RefCell::new(Weak::new()),
            children: RefCell::new(vec![]),
            path: Pathlib::new(full_path)
        })
    }
    pub fn remove_child(&self, value: &T) {
        // Retain only those children that do not match the specified value
        self.children.borrow_mut().retain(|child| child.value != *value);
    }
    fn add_child(parent: &Rc<TreeNode<T>>, child: &Rc<TreeNode<T>>) {
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

    pub fn print(node: &Rc<TreeNode<T>>, indent: usize) {
        println!("{}{:?}", "| ".repeat(indent), node.value);
        for child in node.children.borrow().iter() {
            TreeNode::print(child, indent + 1)
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




pub fn update_from_sarc_paths(root_node: &Rc<TreeNode<String>>, sarc_file: &PackFile) {
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

pub fn update_from_paths(node: &Rc<TreeNode<String>>, paths: Vec<String>) {
    for path in paths {
        let elements: Vec<&str> = path.split("/").collect();
        update_root_from_list(&node, elements, path.clone());
    }
}

fn update_root_from_list(root_node: &Rc<TreeNode<String>>, elements: Vec<&str>, path: String) {
    let mut cur_node: Rc<TreeNode<String>> = Rc::clone(root_node);
    let size: usize = elements.len();
    for (i, elem) in elements.iter().enumerate() {
        if let Some(node) = TreeNode::is_value_in_children(&cur_node, &elem.to_string()) {
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
            //println!("Added node name {} full path {}", &elem, &cur_path);
            let child = TreeNode::new(elem.to_string(), cur_path);
            TreeNode::add_child(&cur_node, &child);
            cur_node = Rc::clone(&child);
        }
    }
}

