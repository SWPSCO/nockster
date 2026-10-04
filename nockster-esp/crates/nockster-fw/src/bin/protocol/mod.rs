use cobs::encode;
use nockster_core::{Msg, Response, ERR_ENCODE_TOO_BIG, PROTO_V1};
use usb_device::bus::UsbBus;
use usbd_hid::hid_class::HIDClass;

use crate::usb_hid;

pub fn send_err<B: UsbBus>(hid: &mut HIDClass<'_, B>, code: u16, enc: &mut [u8]) {
    let msg = Msg {
        v: PROTO_V1,
        id: 0,
        msg: Response::Err { code },
    };
    let mut tmp = [0u8; 64];
    if let Ok(used) = postcard::to_slice(&msg, &mut tmp) {
        let n = encode(used, enc);
        enc[n] = 0;
        let _ = usb_hid::write(hid, &enc[..n + 1]);
    }
}

pub fn send_response<B: UsbBus>(
    hid: &mut HIDClass<'_, B>,
    msg_id: u32,
    resp: Response,
    plain: &mut [u8],
    enc: &mut [u8],
) {
    let msg = Msg {
        v: PROTO_V1,
        id: msg_id,
        msg: resp,
    };
    if send_msg(hid, &msg, plain, enc).is_err() {
        send_err(hid, ERR_ENCODE_TOO_BIG, enc);
    }
}

pub fn send_msg<B: UsbBus>(
    hid: &mut HIDClass<'_, B>,
    msg: &Msg<Response>,
    plain: &mut [u8],
    enc: &mut [u8],
) -> Result<(), ()> {
    let used = postcard::to_slice(msg, plain).map_err(|_| ())?;
    let n = encode(used, enc);
    enc[n] = 0;
    usb_hid::write(hid, &enc[..n + 1]).then_some(()).ok_or(())
}
