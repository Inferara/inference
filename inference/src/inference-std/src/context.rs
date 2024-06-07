use crate::address::Address;

pub struct Context {
    pub name: String,
    pub type_name: String,
    pub mem: Vec<Address>,
    pub max_mem: usize,
}

impl Context {
    pub fn new(name: String, type_name: String, mem: Vec<Address>, max_mem: usize) -> Self {
        Self {
            name,
            type_name,
            mem,
            max_mem,
        }
    }
}
