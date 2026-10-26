//! The application: state, file and edit actions, keyboard shortcuts and the layout of the
//! window (same arrangement as EdenBord: top bar, context bar, tools on the left, objects and
//! properties on the right, status bar, canvas in the middle).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, Key, Modifiers};
use kurbo::{Point, Rect};

use crate::canvas::{CanvasTexture, View};
use crate::clipboard::{self, Clipboard};
use crate::document::{CanvasOp, Document, Kind, Object, ObjectId, Rgba, Style};
use crate::export::{self, Encoded};
use crate::file_format;
use crate::history::Session;
use crate::i18n::{self, fill, t};
use crate::prefs::{self, Prefs};
use crate::render::Renderer;
use crate::theme::{self, pal};
use crate::tools::crop::CropState;
use crate::tools::text::TextEditing;
use crate::tools::{Tool, ToolStyle};
use crate::ui::dialogs::Dialog;

pub const APP: &str = "paint1";
pub const APP_ID: &str = "paint1";
pub const ICON: &[u8] = include_bytes!("../assets/paint1-256.png");

const DEFAULT_SIZE: (u32, u32) = (1280, 720);

/// What the user is doing with the pointer on the canvas.
pub enum Gesture {
    None,
    Pan,
    /// Moving, scaling or rotating the selection; `orig` are the objects before.
    Transform {
        kind: TransformKind,
        start: Point,
        bounds: Rect,
        orig: Vec<Object>,
    },
    /// Dragging an end of a line or arrow (`b` = the second end).
    Endpoint {
        id: ObjectId,
        b: bool,
    },
    /// Selecting by area.
    Marquee {
        start: Point,
        end: Point,
        additive: bool,
    },
    /// Drawing a new object.
    Draw {
        id: ObjectId,
        start: Point,
    },
    Crop,
    /// Drawing the area of a rectangle or free selection over image `target`.
    Area {
        target: ObjectId,
        points: Vec<Point>,
        rect: bool,
    },
}

#[derive(Clone, Copy, PartialEq)]
pub enum TransformKind {
    Move,
    /// Index of the handle, see `tools::select::HANDLES`.
    Scale(usize),
    Rotate,
}

/// Action waiting for the user to confirm discarding unsaved changes.
pub enum Pending {
    New(u32, u32, Rgba),
    Open(PathBuf),
    Quit,
}

pub struct App {
    pub session: Session,
    pub renderer: Renderer,
    pub view: View,
    pub texture: Option<CanvasTexture>,
    pub checker: Option<(egui::TextureHandle, egui::Color32)>,
    pub logo: Option<egui::TextureHandle>,
    pub tool: Tool,
    pub style: ToolStyle,
    pub gesture: Gesture,
    pub crop: CropState,
    pub text_edit: Option<TextEditing>,
    /// Polygon being drawn.
    pub polygon: Option<ObjectId>,
    /// Piece cut out by an area selection, and the revision right after cutting it: if
    /// nothing changed when it is let go, the cut is undone.
    pub floating: Option<(ObjectId, u64)>,
    pub clipboard: Clipboard,
    pub dialog: Option<Dialog>,
    pub pending: Option<Pending>,
    pub message: Option<(String, bool)>,
    pub prefs: Prefs,
    pub export_job: Option<Receiver<Result<(PathBuf, Encoded), String>>>,
    pub pointer_doc: Option<Point>,
    /// Object being renamed in the list, and the name so far.
    pub rename: Option<(ObjectId, String)>,
    pub allow_close: bool,
    title: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, initial: Option<PathBuf>) -> App {
        let prefs: Prefs = cc
            .storage
            .and_then(|s| eframe::get_value(s, prefs::KEY))
            .unwrap_or_default();
        i18n::set_lang(prefs.lang);
        theme::install(&cc.egui_ctx, prefs.theme);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        cc.egui_ctx.set_zoom_factor(prefs.ui_zoom.clamp(0.5, 3.0));
        let (w, h) = DEFAULT_SIZE;
        let mut app = App {
            session: Session::new(Document::new(w, h, Rgba::WHITE), None),
            renderer: Renderer::default(),
            view: View::default(),
            texture: None,
            checker: None,
            logo: load_logo(&cc.egui_ctx),
            tool: Tool::Pencil,
            style: prefs.style.clone(),
            gesture: Gesture::None,
            crop: CropState::default(),
            text_edit: None,
            polygon: None,
            floating: None,
            clipboard: Clipboard::default(),
            dialog: None,
            pending: None,
            message: None,
            prefs,
            export_job: None,
            pointer_doc: None,
            rename: None,
            allow_close: false,
            title: String::new(),
        };
        if let Some(path) = initial {
            app.open_path(&path);
        }
        app
    }

    pub fn doc(&self) -> &Document {
        self.session.doc()
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), false));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), true));
    }

    pub fn doc_name(&self) -> String {
        self.session
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| t().untitled.to_string())
    }

    // ---------------------------------------------------------------- tools

    pub fn set_tool(&mut self, tool: Tool) {
        if self.tool == tool {
            return;
        }
        self.finish_text();
        self.finish_polygon();
        self.gesture = Gesture::None;
        if tool == Tool::Crop {
            self.reset_crop();
        }
        if tool != Tool::Select && tool != Tool::Crop {
            self.session.selection.clear();
        }
        self.tool = tool;
    }

    /// Ends whatever is in progress on the canvas (before an action that changes the doc).
    pub fn settle(&mut self) {
        self.finish_text();
        self.finish_polygon();
        self.gesture = Gesture::None;
        self.session.end_group();
    }

    // ---------------------------------------------------------------- files

    pub fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.settle();
        self.session = Session::new(doc, path);
        self.texture = None;
        self.view.fit_pending = true;
        self.renderer = Renderer::default();
        if self.tool == Tool::Crop {
            self.reset_crop();
        }
    }

    /// Asks to confirm if there are unsaved changes, then does `action`.
    pub fn request(&mut self, action: Pending) {
        if self.session.dirty {
            self.pending = Some(action);
        } else {
            self.run_pending(action);
        }
    }

    pub fn run_pending(&mut self, action: Pending) {
        match action {
            Pending::New(w, h, bg) => self.replace_document(Document::new(w, h, bg), None),
            Pending::Open(path) => self.open_path(&path),
            Pending::Quit => self.allow_close = true,
        }
    }

    /// Opens a drawing or an image as a new document.
    pub fn open_path(&mut self, path: &Path) {
        let is_paint1 = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(file_format::EXTENSION));
        let result = if is_paint1 {
            file_format::load(path).map(|d| (d, Some(path.to_path_buf())))
        } else {
            load_image(path).map(|img| {
                let mut d = Document::new(img.width(), img.height(), Rgba::TRANSPARENT);
                d.add_image(Arc::new(img), Point::ZERO);
                (d, None)
            })
        };
        match result {
            Ok((doc, saved_path)) => {
                self.replace_document(doc, saved_path);
                self.prefs.add_recent(path.to_path_buf());
                self.message = None;
            }
            Err(e) => self.error(fill(t().open_failed, &[&path.display(), &e])),
        }
    }

    /// Open from the dialog or a drop: a drawing replaces the document; an image too if the
    /// document is untouched, otherwise it is added as an object.
    pub fn open_or_insert(&mut self, path: PathBuf) {
        let is_paint1 = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(file_format::EXTENSION));
        if is_paint1 || self.session.is_pristine() {
            self.request(Pending::Open(path));
        } else {
            self.insert_image_file(&path);
        }
    }

    pub fn insert_image_file(&mut self, path: &Path) {
        match load_image(path) {
            Ok(img) => self.insert_image(img),
            Err(e) => self.error(fill(t().open_failed, &[&path.display(), &e])),
        }
    }

    /// Adds an image object centered on the canvas.
    pub fn insert_image(&mut self, img: image::RgbaImage) {
        self.settle();
        let d = self.doc();
        let at = Point::new(
            ((d.width as f64 - img.width() as f64) / 2.0).round(),
            ((d.height as f64 - img.height() as f64) / 2.0).round(),
        );
        let id = self.session.edit(|d| d.add_image(Arc::new(img), at));
        self.session.select_only(id);
        self.tool = Tool::Select;
    }

    pub fn pick_open(&mut self) {
        let t = t();
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(t.filter_supported, &supported_extensions(true))
            .add_filter(t.filter_paint1, &[file_format::EXTENSION])
            .add_filter(t.filter_images, &supported_extensions(false))
            .pick_file()
        {
            self.open_or_insert(path);
        }
    }

    pub fn pick_insert_image(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(t().filter_images, &supported_extensions(false))
            .pick_file()
        {
            self.insert_image_file(&path);
        }
    }

    /// Saves to the current file, or asks where (`as_new` always asks).
    pub fn save(&mut self, as_new: bool) -> bool {
        self.settle();
        let path = match (&self.session.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let name = format!(
                    "{}.{}",
                    self.session
                        .path
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .map_or_else(
                            || t().untitled.to_string(),
                            |s| s.to_string_lossy().into_owned()
                        ),
                    file_format::EXTENSION
                );
                let Some(mut p) = rfd::FileDialog::new()
                    .add_filter(t().filter_paint1, &[file_format::EXTENSION])
                    .set_file_name(name)
                    .save_file()
                else {
                    return false;
                };
                if p.extension()
                    .is_none_or(|e| !e.eq_ignore_ascii_case(file_format::EXTENSION))
                {
                    p.set_extension(file_format::EXTENSION);
                }
                p
            }
        };
        match file_format::save(self.doc(), &path) {
            Ok(()) => {
                self.session.dirty = false;
                self.info(fill(t().saved, &[&path.display()]));
                self.prefs.add_recent(path.clone());
                self.session.path = Some(path);
                true
            }
            Err(e) => {
                self.error(fill(t().save_failed, &[&e]));
                false
            }
        }
    }

    /// Asks where and exports in the background.
    pub fn start_export(&mut self, settings: export::Settings) {
        self.settle();
        let ext = settings.format.extension();
        let stem = self
            .session
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or_else(
                || t().untitled.to_string(),
                |s| s.to_string_lossy().into_owned(),
            );
        let Some(mut path) = rfd::FileDialog::new()
            .add_filter(settings.format.label(), &[ext])
            .set_file_name(format!("{stem}.{ext}"))
            .save_file()
        else {
            return;
        };
        if path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(export::Format::from_extension)
            .is_none_or(|f| f.extension() != ext)
        {
            path.set_extension(ext);
        }
        self.prefs.export = settings.clone();
        let doc = self.doc().clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = export::export(&doc, &settings, &path).map(|e| (path, e));
            let _ = tx.send(r);
        });
        self.export_job = Some(rx);
        self.info(t().exporting);
    }

    fn poll_export(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.export_job else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok((path, e))) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut size = human_size(e.bytes.len() as u64);
                if let Some(q) = e.quality {
                    size = format!("{size} · {}", fill(t().exported_quality, &[&q]));
                }
                self.info(fill(t().exported, &[&name, &size, &e.width, &e.height]));
                self.export_job = None;
            }
            Ok(Err(e)) => {
                self.error(fill(t().export_failed, &[&e]));
                self.export_job = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.export_job = None,
        }
    }

    // ---------------------------------------------------------------- edit

    pub fn undo(&mut self) {
        self.settle();
        self.session.undo();
    }

    pub fn redo(&mut self) {
        self.settle();
        self.session.redo();
    }

    pub fn delete_selection(&mut self) {
        let ids = self.session.selection.clone();
        if ids.is_empty() {
            return;
        }
        self.settle();
        self.session.edit(|d| {
            d.objects.retain(|o| !ids.contains(&o.id));
            d.prune_assets();
        });
        self.session.selection.clear();
    }

    pub fn duplicate_selection(&mut self) {
        let ids = self.selection_in_order();
        if ids.is_empty() {
            return;
        }
        self.settle();
        let new = self.session.edit(|d| {
            let mut new = Vec::new();
            for id in &ids {
                if let Some(o) = d.get(*id).cloned() {
                    let mut o = o;
                    o.id = d.new_id();
                    o.apply(kurbo::Affine::translate((12.0, 12.0)));
                    new.push(o.id);
                    d.objects.push(o);
                }
            }
            new
        });
        self.session.selection = new;
    }

    /// Selected ids, back to front.
    pub fn selection_in_order(&self) -> Vec<ObjectId> {
        self.doc()
            .objects
            .iter()
            .filter(|o| self.session.is_selected(o.id))
            .map(|o| o.id)
            .collect()
    }

    pub fn select_all(&mut self) {
        self.settle();
        self.session.selection = self
            .doc()
            .objects
            .iter()
            .filter(|o| o.visible && !o.locked)
            .map(|o| o.id)
            .collect();
        self.tool = Tool::Select;
    }

    /// Turns the selection into one locked image.
    pub fn flatten_selection(&mut self) {
        let ids = self.selection_in_order();
        if ids.is_empty() {
            return;
        }
        self.settle();
        let name = t().kind_flattened;
        let renderer = &mut self.renderer;
        let id = self
            .session
            .edit(|d| crate::raster::flatten(d, renderer, &ids, name));
        match id {
            Some(id) => self.session.select_only(id),
            None => self.session.revert_last(),
        }
    }

    /// Lets go of the floating piece once it is no longer the only thing selected: if it
    /// was not changed, the cut is undone and the image is whole again.
    pub(crate) fn settle_floating(&mut self) {
        let Some((id, revision)) = self.floating else {
            return;
        };
        let still = self.session.selection.as_slice() == [id]
            && matches!(
                self.tool,
                Tool::Select | Tool::RectSelect | Tool::FreeSelect
            )
            && self.doc().get(id).is_some();
        if still {
            return;
        }
        self.floating = None;
        if self.session.revision() == revision {
            self.session.revert_last();
        }
    }

    pub fn copy(&mut self, ctx: &egui::Context) -> bool {
        let ids = self.selection_in_order();
        if ids.is_empty() {
            return false;
        }
        let img = self
            .clipboard
            .copy(self.session.doc(), &ids, &mut self.renderer);
        if let Some(img) = img {
            ctx.copy_image(img);
            self.info(t().copied);
        }
        true
    }

    pub fn cut(&mut self, ctx: &egui::Context) {
        if self.copy(ctx) {
            self.delete_selection();
        }
    }

    pub fn paste_objects(&mut self) {
        if !self.clipboard.has_objects() {
            self.info(t().nothing_to_paste);
            return;
        }
        self.settle();
        let ids = self.session.edit(|d| self.clipboard.paste(d));
        self.session.selection = ids;
        self.tool = Tool::Select;
    }

    fn paste_image(&mut self, img: &egui::ColorImage) {
        if self.clipboard.is_ours(img) {
            self.paste_objects();
        } else if let Some(rgba) = clipboard::to_rgba(img) {
            if self.session.is_pristine() {
                let mut d = Document::new(rgba.width(), rgba.height(), Rgba::WHITE);
                d.add_image(Arc::new(rgba), Point::ZERO);
                self.replace_document(d, None);
                self.session.dirty = true;
            } else {
                self.insert_image(rgba);
            }
        }
    }

    /// Text from the clipboard becomes a text object in the middle of the canvas.
    fn paste_text(&mut self, text: &str) {
        let text = text.trim_end();
        if text.is_empty() {
            return;
        }
        self.settle();
        let style = self.style.for_tool(Tool::Text);
        let (size, bold, align) = (self.style.font_size, self.style.bold, self.style.align);
        let id = self.session.edit(|d| {
            let m = crate::text::measure(text, size, bold);
            let at = Point::new(
                ((d.width as f64 - m.width) / 2.0).round(),
                ((d.height as f64 - m.height) / 2.0).round(),
            );
            add_object(
                d,
                Kind::Text {
                    content: text.to_string(),
                    size,
                    bold,
                    align,
                },
                style,
                kurbo::Affine::translate(at.to_vec2()),
            )
        });
        self.session.select_only(id);
        self.tool = Tool::Select;
    }

    /// Rotate or flip: the selection if there is one, the whole image if not.
    pub fn transform_op(&mut self, op: CanvasOp) {
        self.settle();
        let ids = self.session.selection.clone();
        if ids.is_empty() {
            self.session.edit(|d| d.canvas_op(op));
            if matches!(op, CanvasOp::RotateLeft | CanvasOp::RotateRight) {
                self.view.fit_pending = true;
            }
            if self.tool == Tool::Crop {
                self.reset_crop();
            }
        } else {
            self.session.edit(|d| d.objects_op(&ids, op));
        }
    }

    pub fn fit_canvas_to_selection(&mut self) {
        let Some(b) = self.doc().bounds_of(&self.session.selection) else {
            return;
        };
        self.settle();
        self.session.edit(|d| d.crop(b));
        self.view.fit_pending = true;
        if self.tool == Tool::Crop {
            self.reset_crop();
        }
    }

    /// Moves an object in the stacking order (`to` is the final index, back to front).
    pub fn reorder(&mut self, from: usize, to: usize) {
        self.settle();
        self.session.edit(|d| {
            if from < d.objects.len() {
                let o = d.objects.remove(from);
                d.objects.insert(to.min(d.objects.len()), o);
            }
        });
    }

    /// Raises (`up`) or lowers the selection one step, or to the end (`all_the_way`).
    pub fn restack(&mut self, up: bool, all_the_way: bool) {
        let ids = self.session.selection.clone();
        if ids.is_empty() {
            return;
        }
        self.settle();
        self.session.edit(|d| {
            if all_the_way {
                let (picked, rest): (Vec<Object>, Vec<Object>) =
                    d.objects.drain(..).partition(|o| ids.contains(&o.id));
                d.objects = if up {
                    rest.into_iter().chain(picked).collect()
                } else {
                    picked.into_iter().chain(rest).collect()
                };
            } else {
                step_objects(&mut d.objects, &ids, up);
            }
        });
    }

    // ---------------------------------------------------------------- frame

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for path in dropped {
            self.open_or_insert(path);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let dirty = if self.session.dirty { " •" } else { "" };
        let title = format!("{}{dirty} — {APP}", self.doc_name());
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.session.dirty
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Clipboard and keyboard shortcuts (not while typing in a field).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.memory(|m| m.focused()).is_some();
        if typing {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for e in &events {
            match e {
                egui::Event::Copy => {
                    self.copy(ctx);
                }
                egui::Event::Cut => self.cut(ctx),
                egui::Event::Paste(text) => self.paste_text(text),
                egui::Event::PasteImage(img) => self.paste_image(img),
                _ => {}
            }
        }
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let pressed =
            |ctx: &egui::Context, m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(ctx, cmd_shift, Key::F) {
            self.flatten_selection();
        }
        if pressed(ctx, cmd_shift, Key::S) {
            self.save(true);
        }
        if pressed(ctx, cmd_shift, Key::Z) || pressed(ctx, cmd, Key::Y) {
            self.redo();
        }
        if pressed(ctx, cmd, Key::Z) {
            self.undo();
        }
        if pressed(ctx, cmd, Key::S) {
            self.save(false);
        }
        if pressed(ctx, cmd, Key::O) {
            self.pick_open();
        }
        if pressed(ctx, cmd, Key::N) {
            self.dialog = Some(Dialog::new_image(self.doc()));
        }
        if pressed(ctx, cmd, Key::E) {
            self.dialog = Some(Dialog::export(self.prefs.export.clone()));
        }
        if pressed(ctx, cmd, Key::D) {
            self.duplicate_selection();
        }
        if pressed(ctx, cmd, Key::A) {
            self.select_all();
        }
        if pressed(ctx, cmd, Key::Num0) {
            self.view.fit_pending = true;
        }
        if pressed(ctx, cmd, Key::Plus) || pressed(ctx, cmd, Key::Equals) {
            self.view.zoom_by(1.25);
        }
        if pressed(ctx, cmd, Key::Minus) {
            self.view.zoom_by(0.8);
        }
        if pressed(ctx, Modifiers::NONE, Key::Delete)
            || pressed(ctx, Modifiers::NONE, Key::Backspace)
        {
            self.delete_selection();
        }
        if pressed(ctx, Modifiers::NONE, Key::Escape) {
            self.escape();
        }
        if pressed(ctx, Modifiers::NONE, Key::Enter) {
            if self.tool == Tool::Crop {
                self.apply_crop();
            } else if self.polygon.is_some() {
                self.finish_polygon();
            }
        }
        // Arrows nudge the selection (Shift: 10 px).
        if !self.session.selection.is_empty() {
            let mut d = kurbo::Vec2::ZERO;
            for (key, v) in [
                (Key::ArrowLeft, (-1.0, 0.0)),
                (Key::ArrowRight, (1.0, 0.0)),
                (Key::ArrowUp, (0.0, -1.0)),
                (Key::ArrowDown, (0.0, 1.0)),
            ] {
                for (m, step) in [(Modifiers::NONE, 1.0), (Modifiers::SHIFT, 10.0)] {
                    if pressed(ctx, m, key) {
                        d += kurbo::Vec2::new(v.0, v.1) * step;
                    }
                }
            }
            if d != kurbo::Vec2::ZERO {
                let ids = self.session.selection.clone();
                self.session.edit_grouped("nudge", |doc| {
                    doc.transform_objects(&ids, kurbo::Affine::translate(d))
                });
            }
        }
        let tool = ctx.input(|i| {
            if i.modifiers.any() {
                return None;
            }
            i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key, pressed: true, ..
                } => Tool::shortcut(*key),
                _ => None,
            })
        });
        if let Some(tool) = tool {
            self.set_tool(tool);
        }
    }

    pub fn escape(&mut self) {
        if !matches!(self.gesture, Gesture::None) {
            self.gesture = Gesture::None;
            self.session.cancel_group();
        } else if self.polygon.is_some() {
            self.polygon = None;
            self.session.cancel_group();
        } else if self.tool == Tool::Crop {
            self.set_tool(Tool::Select);
        } else {
            self.session.selection.clear();
        }
    }
}

/// Moves each selected object one place up or down past an unselected neighbor.
fn step_objects(objects: &mut [Object], ids: &[ObjectId], up: bool) {
    let n = objects.len();
    if up {
        for i in (0..n.saturating_sub(1)).rev() {
            if ids.contains(&objects[i].id) && !ids.contains(&objects[i + 1].id) {
                objects.swap(i, i + 1);
            }
        }
    } else {
        for i in 1..n {
            if ids.contains(&objects[i].id) && !ids.contains(&objects[i - 1].id) {
                objects.swap(i, i - 1);
            }
        }
    }
}

pub fn add_object(
    d: &mut Document,
    kind: Kind,
    style: Style,
    transform: kurbo::Affine,
) -> ObjectId {
    let id = d.new_id();
    d.objects.push(Object::new(id, kind, style, transform));
    id
}

pub fn default_text_kind(style: &ToolStyle) -> Kind {
    Kind::Text {
        content: String::new(),
        size: style.font_size,
        bold: style.bold,
        align: style.align,
    }
}

fn load_image(path: &Path) -> Result<image::RgbaImage, String> {
    image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map(|i| i.into_rgba8())
        .map_err(|e| e.to_string())
}

fn supported_extensions(with_paint1: bool) -> Vec<&'static str> {
    let mut v = vec!["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];
    if with_paint1 {
        v.insert(0, file_format::EXTENSION);
    }
    v
}

pub fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.0} KB", (bytes as f64 / 1024.0).ceil())
    }
}

fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(ICON).ok()?.into_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    );
    Some(ctx.load_texture("logo", color, egui::TextureOptions::LINEAR))
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        theme::sync(ctx);
        self.handle_drops(ctx);
        if self.pending.is_none() && self.dialog.is_none() {
            // While typing on the canvas, keys belong to the text.
            if self.text_edit.is_some() {
                self.text_keys(ctx);
            } else {
                self.shortcuts(ctx);
            }
        }
        // A grouped edit (dragging a value, a gesture) ends when the mouse is released and
        // no field has focus; typing on the canvas and drawing a polygon also group.
        let idle = ctx.input(|i| !i.pointer.any_down())
            && ctx.memory(|m| m.focused()).is_none()
            && self.text_edit.is_none()
            && self.polygon.is_none()
            && matches!(self.gesture, Gesture::None);
        if idle {
            self.session.end_group();
        }
        self.settle_floating();
        self.poll_export(ctx);
        self.update_title(ctx);
        self.handle_close(ctx);
        self.prefs.ui_zoom = ctx.zoom_factor();
        self.prefs.style = self.style.clone();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, prefs::KEY, &self.prefs);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let p = pal();
        let bar = egui::Frame::NONE
            .fill(p.panel)
            .inner_margin(egui::Margin::symmetric(10, 6));
        let line = egui::Stroke::new(1.0, p.border);
        egui::Panel::top("top_bar")
            .frame(bar)
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::top("context_bar")
            .frame(
                egui::Frame::NONE
                    .fill(p.window)
                    .stroke(line)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| self.context_bar(ui));
        egui::Panel::bottom("status_bar")
            .frame(bar)
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(46.0)
            .frame(egui::Frame::NONE.fill(p.panel).inner_margin(6))
            .show(ui, |ui| self.tools_bar(ui));
        egui::Panel::right("side_panel")
            .resizable(true)
            .default_size(290.0)
            .min_size(230.0)
            .frame(
                egui::Frame::NONE
                    .fill(p.panel)
                    .inner_margin(egui::Margin::symmetric(10, 0)),
            )
            .show(ui, |ui| self.side_panel(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(p.backdrop))
            .show(ui, |ui| self.canvas(ui));
        self.dialogs(&ctx);
        self.confirm_dialog(&ctx);
    }
}
