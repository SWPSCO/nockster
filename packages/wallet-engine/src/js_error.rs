use wasm_bindgen::prelude::*;
// Tiny helpers so you can pick a proper JS error class.
pub fn js_error(msg: &str) -> JsValue {
    js_sys::Error::new(msg).into()
}
#[allow(dead_code)]
pub fn type_error(msg: &str) -> JsValue {
    js_sys::TypeError::new(msg).into()
}
#[allow(dead_code)]
pub fn range_error(msg: &str) -> JsValue {
    js_sys::RangeError::new(msg).into()
}
