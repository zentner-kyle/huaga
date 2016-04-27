extern crate gtk;
extern crate gdk;
extern crate gdk_pixbuf;

use std::path::{PathBuf};
use std::sync::{Mutex};
use gtk::prelude::*;

use gtk::{Button, Window, WindowType};
use gdk_pixbuf::{Pixbuf};

struct ViewState {
    zoom: f64,
    applied_zoom: f64,
    pixbuf: Option<Pixbuf>,
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
        zoom: 1.0,
        applied_zoom: 1.0,
        pixbuf: None,
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
                    view.pixbuf = Some(loaded_pixbuf.clone());
                    update_image(&view.image, &loaded_pixbuf, view.zoom).ok();
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
            view.zoom = view.zoom - evt.get_delta().1 * 0.02;
            if view.zoom < 0.1f64 {
                view.zoom = 0.1f64;
            }
            if view.zoom > 3f64 {
                view.zoom = 3f64;
            }
            gtk::Inhibit(true)
        } else {
            gtk::Inhibit(false)
        }
    });

    let view3 = view;
    gtk::timeout_add(20, move || {
        let mut view = view3.lock().unwrap();
        if view.applied_zoom != view.zoom {
            let image = view.image.clone();
            if let Some(ref pixbuf) = view.pixbuf {
                update_image(&image, &pixbuf, view.zoom).ok();
            }
            view.applied_zoom = view.zoom;
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

fn load_pixbuf(filepath: Option<PathBuf>) -> Result<Pixbuf, ()> {
    let filepath = try!(filepath.ok_or(()));
    let filename = try!(filepath.to_str().ok_or(()));
    let loaded = try!(Pixbuf::new_from_file(filename).map_err(|_| ()));
    Ok(loaded.clone())
}

fn update_image(image: &gtk::Image, pixbuf: &Pixbuf, zoom: f64) -> Result<(), ()> {
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
