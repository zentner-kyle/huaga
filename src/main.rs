extern crate gtk;
extern crate gdk;
extern crate glib;
extern crate gdk_pixbuf;

use std::cmp;
use std::f64;
use std::i32;
use std::path::{PathBuf};
use std::sync::{Mutex};

use gtk::prelude::*;

use gtk::{Window, WindowType};
use gdk_pixbuf::{Pixbuf, PixbufAnimation, PixbufAnimationIter, PixbufAnimationExt};

struct ViewState {
    image_min_size: i32,
    image_dirty: bool,
    image_path: Option<PathBuf>,
    pixbuf: Option<Pixbuf>,
    animated_pixbuf: Option<PixbufAnimation>,
    animated_pixbuf_iter: Option<PixbufAnimationIter>,
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

    let event_box = gtk::EventBox::new();
    event_box.add(&scroll);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.pack_start(&toolbar, false, true, 0);
    vbox.pack_start(&event_box, true, true, 0);

    window.add(&vbox);

    let view = static_mutex(ViewState{
        image_min_size: 0i32,
        image_dirty: false,
        image_path: None,
        pixbuf: None,
        animated_pixbuf: None,
        animated_pixbuf_iter: None,
        image: image.clone(),
    });

    let window1 = window.clone();
    open_button.connect_clicked(move |_| {
        let file_chooser = gtk::FileChooserDialog::new(
            Some("Open File"), Some(&window1), gtk::FileChooserAction::Open);
        file_chooser.add_buttons(&[
            ("Open", gtk::ResponseType::Ok as i32),
            ("Cancel", gtk::ResponseType::Cancel as i32),
        ]);
        if file_chooser.run() == gtk::ResponseType::Ok as i32 {
            let mut view = view.lock().unwrap();
            do_load_pixbuf(&mut view, &file_chooser.get_filename()).ok();
        }

        file_chooser.destroy();
    });

    scroll.connect_scroll_event(move |_, evt| {
        if evt.get_state().contains(gdk::enums::modifier_type::ControlMask) {
            let mut view = view.lock().unwrap();
            let mut delta = -evt.get_delta().1;
            if delta == 0f64 {
                if evt.as_ref().direction == gdk::ScrollDirection::Down {
                    delta = 50f64;
                } else if evt.as_ref().direction == gdk::ScrollDirection::Up {
                    delta = -50f64;
                } else {
                    return gtk::Inhibit(false);
                }
            }
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
                let new_image_min_size = clip_f64((std::i32::MIN + 1i32) as f64, view.image_min_size as f64 + diff, std::i32::MAX as f64);
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

    gtk::timeout_add(20, move || {
        let mut view = view.lock().unwrap();
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

    event_box.connect_button_press_event(move |_, evt| {
        let mut view = view.lock().unwrap();
        if evt.get_event_type() == gdk::EventType::ButtonPress {
            if evt.as_ref().button == 3 || evt.get_state().contains(gdk::enums::modifier_type::ShiftMask) {
                if let Err(e) = load_prev_image(&mut view) {
                    println!("Error loading image: {}", e);
                }
            } else {
                if let Err(e) = load_next_image(&mut view) {
                    println!("Error loading image: {}", e);
                }
            }
            if let Some(adjustment) = scroll.get_vadjustment() {
                adjustment.set_value(0f64);
            }
            Inhibit(true)
        } else {
            Inhibit(false)
        }
    });

    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        Inhibit(false)
    });

    window.show_all();
    gtk::main();
}

fn nearby_files(image_path: &Option<PathBuf>) -> Result<Vec<PathBuf>, String> {
    let image_path = try!(image_path.clone().ok_or("Bad initial image path".to_owned()));
    let parent = try!(image_path.parent().ok_or("image doesn't exist in directory".to_owned()));
    let dir = try!(std::fs::read_dir(parent).map_err(|_| "Could not open directory for reading".to_owned()));
    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut found = false;
    for entry in dir {
        let entry = try!(entry.map_err(|_| "bad directory entry".to_owned()));
        let path = entry.path();
        if path == image_path {
            found = true;
            before.push(path);
            continue;
        }
        if found {
            after.push(path);
        } else {
            before.push(path);
        }
    }
    after.extend_from_slice(before.as_slice());
    return Ok(after);
}

fn load_next_image(view: &mut ViewState) -> Result<(), String> {
    let nearby = try!(nearby_files(&view.image_path));
    for file in nearby {
        if let Ok(_) = do_load_pixbuf(view, &Some(file)) {
            return Ok(());
        }
    }
    return Err("Could not load any files.".to_owned());
}

fn load_prev_image(view: &mut ViewState) -> Result<(), String> {
    let nearby = try!(nearby_files(&view.image_path));
    for i in 0..nearby.len() {
        let file = nearby[(2 * nearby.len() - i - 2) % nearby.len()].clone();
        if let Ok(_) = do_load_pixbuf(view, &Some(file)) {
            return Ok(());
        }
    }
    return Err("Could not load any files.".to_owned());
}


fn do_load_pixbuf(view: &mut ViewState, path: &Option<PathBuf>) -> Result<(), ()> {
    let loaded_pixbuf = { try!(load_pixbuf(&path)) };
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
    view.image_path = path.clone();
    if view.image_min_size == 0 {
        let min_size = { pixbuf_min_size(&view.pixbuf.clone().unwrap()) };
        view.image_min_size = min_size;
    }
    update_image(&view.image, &view.pixbuf.clone().unwrap(), view.image_min_size)
}

fn load_pixbuf(filepath: &Option<PathBuf>) -> Result<PixbufAnimation, ()> {
    let filepath = try!(filepath.clone().ok_or(()));
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
