#![no_main]
#![no_std]

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn foo(input: i32) -> i32 {
    input * 2
}
