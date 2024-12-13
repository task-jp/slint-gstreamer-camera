// Prevent console window in addition to Slint window in Windows release builds when, e.g., starting the app via file manager. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{bail, Result};
use gstreamer::prelude::*;
use gstreamer_video::VideoFrameExt;
use std::error::Error;

slint::include_modules!();

fn try_gstreamer_video_frame_to_pixel_buffer(
    frame: &gstreamer_video::VideoFrame<gstreamer_video::video_frame::Readable>,
) -> Result<slint::SharedPixelBuffer<slint::Rgb8Pixel>> {
    match frame.format() {
        gstreamer_video::VideoFormat::Rgb => {
            let mut slint_pixel_buffer =
                slint::SharedPixelBuffer::<slint::Rgb8Pixel>::new(frame.width(), frame.height());
            frame
                .buffer()
                .copy_to_slice(0, slint_pixel_buffer.make_mut_bytes())
                .expect("Unable to copy to slice!"); // Copies!
            Ok(slint_pixel_buffer)
        }
        _ => {
            bail!(
                "Cannot convert frame to a slint RGB frame because it is format {}",
                frame.format().to_str()
            )
        }
    }
}


fn main() -> Result<(), Box<dyn Error>> {
    gstreamer::init().unwrap();

    let ui = AppWindow::new()?;
    let width: i32 = ui.get_window_width() as i32;
    let height: i32 = ui.get_window_height() as i32;
    let app_weak = ui.as_weak();

    println!("Window size: {}x{}", width, height);

    let source = gstreamer::ElementFactory::make("v4l2src")
        .name("source")
        .build()
        .expect("Could not create source element.");
    source.set_property("device", &"/dev/video0");

    let videoconvert = gstreamer::ElementFactory::make("videoconvert")
        .name("convert")
        .build()
        .expect("Failed to create videoconvert element");

    let caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", &"RGB")
        .field("width", width)
        .field("height", height)
        .build();

    let capsfilter = gstreamer::ElementFactory::make("capsfilter")
        .name("filter")
        .build()
        .expect("Failed to create capsfilter element");
    capsfilter.set_property("caps", &caps);

    let appsink = gstreamer_app::AppSink::builder()
        .caps(
            &gstreamer_video::VideoCapsBuilder::new()
                .format(gstreamer_video::VideoFormat::Rgb)
                .width(width)
                .height(height)
                .build(),
        )
        .build();

    let pipeline = gstreamer::Pipeline::with_name("camera-pipeline");

    pipeline.add_many([&source, &videoconvert, &capsfilter, &appsink.upcast_ref()]).unwrap();
    gstreamer::Element::link_many([&source, &videoconvert, &capsfilter, &appsink.upcast_ref()]).unwrap();

    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |appsink| {
                let sample = appsink.pull_sample().map_err(|_| gstreamer::FlowError::Eos)?;
                let buffer = sample.buffer_owned().unwrap(); // Probably copies!
                let video_info =
                    gstreamer_video::VideoInfo::builder(gstreamer_video::VideoFormat::Rgb, width as u32, height as u32)
                        .build()
                        .expect("couldn't build video info!");
                let video_frame =
                    gstreamer_video::VideoFrame::from_buffer_readable(buffer, &video_info).unwrap();
                let slint_frame = try_gstreamer_video_frame_to_pixel_buffer(&video_frame)
                    .expect("Unable to convert the video frame to a slint video frame!");

                app_weak
                    .upgrade_in_event_loop(|app| {
                        app.set_video_frame(slint::Image::from_rgb8(slint_frame))
                    })
                    .unwrap();

                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    pipeline
        .set_state(gstreamer::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    ui.run()?;

    Ok(())
}
