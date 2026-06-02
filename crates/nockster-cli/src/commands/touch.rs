use crate::cli::TouchArgs;
use crate::serial::{open, send_call};
use crate::ui;
use nockster_core::{Request, Response, TouchCalibration};

pub fn run(args: &TouchArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    ui::header("touch");

    if args.calibrate {
        ui::info("touch each target on the device");
        match send_call(&mut *sp, 0x5403, Request::StartTouchCalibration)? {
            Response::OkTouchCalibration(calibration) => {
                ui::ok("wrote touch calibration");
                print_calibration(calibration);
                return Ok(());
            }
            Response::Err { code } => {
                anyhow::bail!("touch calibration failed with code {code}");
            }
            other => anyhow::bail!("unexpected touch calibration response: {other:?}"),
        }
    }

    let current = match send_call(&mut *sp, 0x5400, Request::GetTouchCalibration)? {
        Response::OkTouchCalibration(calibration) => calibration,
        Response::Err { code } => anyhow::bail!("get touch calibration failed with code {code}"),
        other => anyhow::bail!("unexpected touch calibration response: {other:?}"),
    };

    let mut next = current;
    let mut changed = false;
    if let Some(v) = args.x_min {
        next.raw_x_min = v;
        changed = true;
    }
    if let Some(v) = args.x_max {
        next.raw_x_max = v;
        changed = true;
    }
    if let Some(v) = args.y_min {
        next.raw_y_min = v;
        changed = true;
    }
    if let Some(v) = args.y_max {
        next.raw_y_max = v;
        changed = true;
    }
    if let Some(v) = args.mirror_x {
        next.mirror_x = v;
        changed = true;
    }
    if let Some(v) = args.mirror_y {
        next.mirror_y = v;
        changed = true;
    }

    if changed {
        validate(next)?;
        match send_call(
            &mut *sp,
            0x5401,
            Request::SetTouchCalibration { calibration: next },
        )? {
            Response::Ok => ui::ok("wrote touch calibration"),
            Response::Err { code } => {
                anyhow::bail!("set touch calibration failed with code {code}")
            }
            other => anyhow::bail!("unexpected set-touch response: {other:?}"),
        }
    }

    print_calibration(if changed { next } else { current });
    if args.diagnostics || args.exit_diagnostics {
        match send_call(
            &mut *sp,
            0x5402,
            Request::ShowTouchDiagnostics {
                enabled: args.diagnostics,
            },
        )? {
            Response::Ok => {
                if args.diagnostics {
                    ui::ok("touch diagnostics shown on device");
                } else {
                    ui::ok("touch diagnostics hidden on device");
                }
            }
            Response::Err { code } => {
                anyhow::bail!("touch diagnostics request failed with code {code}")
            }
            other => anyhow::bail!("unexpected touch diagnostics response: {other:?}"),
        }
    }
    Ok(())
}

fn validate(calibration: TouchCalibration) -> anyhow::Result<()> {
    if calibration.raw_x_min >= calibration.raw_x_max {
        anyhow::bail!("x-min must be lower than x-max");
    }
    if calibration.raw_y_min >= calibration.raw_y_max {
        anyhow::bail!("y-min must be lower than y-max");
    }
    Ok(())
}

fn print_calibration(calibration: TouchCalibration) {
    ui::kv(
        "x range",
        ui::strong(&format!(
            "{}..{}",
            calibration.raw_x_min, calibration.raw_x_max
        )),
    );
    ui::kv(
        "y range",
        ui::strong(&format!(
            "{}..{}",
            calibration.raw_y_min, calibration.raw_y_max
        )),
    );
    ui::kv("mirror x", ui::yesno(calibration.mirror_x));
    ui::kv("mirror y", ui::yesno(calibration.mirror_y));
}
