

use std::cell::{RefCell};
use std::fmt::Debug;

use std::rc::{Rc, Weak};


use crate::file_format::Pack::PackFile;

use crate::Settings::Pathlib;

pub struct TreeNode<T> {
    pub value: T,
    pub parent: RefCell<Weak<TreeNode<T>>>,
    pub children: RefCell<Vec<Rc<TreeNode<T>>>>,
    pub path: Pathlib,
    //pub is_shown:bool,
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
            path: Pathlib::new(full_path),
            //is_shown: true,
        })
    }

    pub fn clean_up_tree(node: &Rc<Self>, val: &str) {
        let mut children_to_keep = Vec::new();

        // Iterate over children to decide which ones to keep
        for child in node.children.borrow().iter() {
            // Recursively clean up the tree first, so we work our way up from the leaves
            Self::clean_up_tree(child, val);

            let is_leaf_node = child.children.borrow().is_empty();
            let has_asdf = child.path.name.to_lowercase().contains(&val.to_lowercase());
            let has_dot = child.path.name.contains(".");

            // Decide whether to keep the child
            if is_leaf_node && !has_asdf {
                // Do not keep leaf nodes that do not contain "asdf"
                continue;
            }

            if is_leaf_node && !has_dot {
                // Do not keep leaf nodes that do not have "." in their name
                continue;
            }

            // If none of the conditions for removal are met, keep the node
            children_to_keep.push(Rc::clone(child));
        }

        // Replace the children with the filtered list
        *node.children.borrow_mut() = children_to_keep;
    }

    pub fn contains_value_in_any_child(&self, search_value: &str) -> bool {
        if search_value.is_empty() {
            return true;
        }
        for child in self.children.borrow().iter() {
            if child.path.name.to_lowercase().contains(search_value) {
                println!("{}", &child.path.name);
                return true;
            }
            if child.contains_value_in_any_child(search_value) {
                return true;
            }
        }
        false
    }

    pub fn is_shown(&self, text: &str) -> bool {
        text.is_empty() || self.is_leaf() && self.is_file() && self.path.full_path.to_lowercase().contains(&text.to_lowercase())
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

    pub fn is_file(&self) -> bool {
        self.path.full_path.contains(".")
    }

    pub fn is_root(&self) -> bool {
        self.parent.borrow().upgrade().is_none()
    }
    pub fn get_parent(&self) -> Option<Rc<TreeNode<T>>> {
        let parent = self.parent.borrow().upgrade();
        match parent {
            Some(p) => {return Some(Rc::clone(&p));},
            None => {return None;}
        }
    }

    pub fn remove_itself(&self) {
        if let Some(parent) = &self.get_parent() {
            parent.remove_child(&self.value);
        } else {
            println!("Removing root node unsupported")
        }
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
        if let Some(name) = &file.name() {
            paths.push(name.to_string());
        }
    }
    
    //paths.sort(); doesnt sort like windows
   // paths.sort_by(|a, b| compare(&a, &b));
    paths.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
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

