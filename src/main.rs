extern crate gtk;
extern crate gdk;
extern crate glib;
extern crate gdk_pixbuf;

use std::path::{PathBuf};
use std::sync::{Mutex};
use std::cmp;
use std::f64;
use std::i32;
use gtk::prelude::*;

use gtk::{Button, Window, WindowType};
use gdk_pixbuf::{Pixbuf, PixbufAnimation, PixbufAnimationIter, PixbufAnimationExt};

struct ViewState {
    image_min_size: i32,
    image_dirty: bool,
    pixbuf: Option<Pixbuf>,
    animated_pixbuf: Option<PixbufAnimation>,
    animated_pixbuf_iter: Option<PixbufAnimationIter>,
    window: gtk::Window,
    image: gtk::Image,
}

fn static_mutex<T>(t: T) -> &'static Mutex<T> {
    unsafe { &*Box::into_raw(Box::new(Mutex::new(t))) }
}

fn main() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    let window = Window::new(WindowType::Toplevel);
    window.set_title("Huaga");

    let toolbar = gtk::Toolbar::new();

    let image = gtk::Image::new();

    let open_icon = gtk::Image::new_from_icon_name("document-open",
                                                   gtk::IconSize::SmallToolbar as i32);
    let open_button = gtk::ToolButton::new::<gtk::Image>(Some(&open_icon), Some("Open"));

    toolbar.add(&open_button);

    let scroll = gtk::ScrolledWindow::new(None, None);
    scroll.add(&image);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&toolbar, false, true, 0);
    vbox.pack_start(&scroll, true, true, 0);

    window.add(&vbox);

    let view = static_mutex(ViewState{
        image_min_size: 0i32,
        image_dirty: false,
        pixbuf: None,
        animated_pixbuf: None,
        animated_pixbuf_iter: None,
        window: window.clone(),
        image: image.clone(),
    });

    let view1 = view;
    let window1 = window.clone();
    open_button.connect_clicked(move |_| {
        let file_chooser = gtk::FileChooserDialog::new(
            Some("Open File"), Some(&window1), gtk::FileChooserAction::Open);
        file_chooser.add_buttons(&[
            ("Open", gtk::ResponseType::Ok as i32),
            ("Cancel", gtk::ResponseType::Cancel as i32),
        ]);
        if file_chooser.run() == gtk::ResponseType::Ok as i32 {
            match load_pixbuf(file_chooser.get_filename()) {
                Ok(loaded_pixbuf) => {
                    let mut view = view1.lock().unwrap();
                    if loaded_pixbuf.is_static_image() {
                        let pixbuf = loaded_pixbuf.get_static_image().expect("it said it was static");
                        view.pixbuf = Some(pixbuf.clone());
                        view.animated_pixbuf = None;
                        view.animated_pixbuf_iter = None;
                    } else {
                        let iter = loaded_pixbuf.get_iter(&glib::get_current_time());
                        view.animated_pixbuf = Some(loaded_pixbuf.clone());
                        view.animated_pixbuf_iter = Some(iter.clone());
                        view.pixbuf = Some(iter.get_pixbuf());
                    }
                    if view.image_min_size == 0 {
                        let min_size = { pixbuf_min_size(&view.pixbuf.clone().unwrap()) };
                        view.image_min_size = min_size;
                    }
                    update_image(&view.image, &view.pixbuf.clone().unwrap(), view.image_min_size).ok();
                },
                Err(_) => {},
            }
        }

        file_chooser.destroy();
    });

    let view2 = view;
    scroll.connect_scroll_event(move |_, evt| {
        if evt.get_state().contains(gdk::enums::modifier_type::ControlMask) {
            let mut view = view2.lock().unwrap();
            let delta = -evt.get_delta().1;
            if let Some(ref pixbuf) = view.pixbuf.clone() {
                let pixbuf_size = pixbuf_min_size(pixbuf);
                let ratio = pixbuf_size as f64 / view.image_min_size as f64;
                let dratio = delta as f64 / view.image_min_size as f64;
                let min = 0.1f64;
                let max = 4.0f64;
                let dzoom = 0.03 * dzoom_from_dratio(ratio,
                                              dratio,
                                              f64::min(ratio, min),
                                              f64::max(ratio, max));
                let diff = view.image_min_size as f64 * dzoom;
                let new_image_min_size = clip_f64(std::i32::MIN as f64, view.image_min_size as f64 + diff, std::i32::MAX as f64);
                if diff.is_sign_positive() {
                    view.image_min_size = f64::min(max * view.image_min_size as f64, new_image_min_size) as i32;
                } else {
                    view.image_min_size = f64::max(min * view.image_min_size as f64, new_image_min_size) as i32;
                }
                view.image_dirty = true;
            }
            gtk::Inhibit(true)
        } else {
            gtk::Inhibit(false)
        }
    });

    let view3 = view;
    gtk::timeout_add(20, move || {
        let mut view = view3.lock().unwrap();
        let mut must_update = false;
        if let Some(ref iter) = view.animated_pixbuf_iter.clone() {
            if iter.advance(&glib::get_current_time()) {
                view.pixbuf = Some(iter.get_pixbuf());
                must_update = true;
            }
        }
        let image = view.image.clone();
        if view.image_dirty || must_update {
            if let Some(ref pixbuf) = view.pixbuf {
                update_image(&image, &pixbuf, view.image_min_size).ok();
            }
            view.image_dirty = false;
        }
        gtk::Continue(true)
    });

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    window.show_all();
    gtk::main();
}

fn load_pixbuf(filepath: Option<PathBuf>) -> Result<PixbufAnimation, ()> {
    let filepath = try!(filepath.ok_or(()));
    let filename = try!(filepath.to_str().ok_or(()));
    let loaded = try!(PixbufAnimation::new_from_file(filename).map_err(|_| ()));
    Ok(loaded.clone())
}

fn update_image(image: &gtk::Image, pixbuf: &Pixbuf, image_min_size: i32) -> Result<(), ()> {
    let pixbuf_min_size = cmp::max(1, pixbuf_min_size(pixbuf));
    let zoom: f64 = image_min_size as f64 / pixbuf_min_size as f64;
    let scaled_width = (pixbuf.get_width() as f64 * zoom) as i32;
    let scaled_height = (pixbuf.get_height() as f64 * zoom) as i32;
    if scaled_width <= 0 || scaled_height <= 0 {
        return Err(());
    }
    if let Ok(scaled_pixbuf) = pixbuf.scale_simple(scaled_width, scaled_height, gdk_pixbuf::InterpType::Bilinear) {
        image.set_from_pixbuf(Some(&scaled_pixbuf));
        return Ok(());
    } else {
        return Err(());
    }
}

fn pixbuf_min_size(pixbuf: &Pixbuf) -> i32 {
    cmp::min(pixbuf.get_width(), pixbuf.get_height())
}

fn clip_f64(min: f64, val: f64, max: f64) -> f64 {
    f64::min(max, f64::max(min, val))
}

fn dzoom_from_dratio(ratio: f64, dratio: f64, min: f64, max: f64) -> f64 {
    // We would like the differential to become zero at min and max.
    // We'll use an inverted hyperbolic.
    let direction = dratio.signum();
    let mut hyper = 1f64 - 1f64/((ratio - min) * (max - ratio));
    if hyper < 0.1f64 {
        hyper = 0f64;
    }
    // If dratio is pushing us more towards ratio == 1.
    if ((ratio - 1.0f64) * dratio).is_sign_positive() {
        direction
    } else {
        direction * hyper
    }
}
