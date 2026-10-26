//! The open document with its file, selection and snapshot-based undo/redo.

use std::path::PathBuf;

use crate::document::{Document, ObjectId};

/// Undo steps kept.
const HISTORY_LIMIT: usize = 100;

pub struct Session {
    doc: Document,
    undo: Vec<Document>,
    redo: Vec<Document>,
    /// Key of the grouped edit in progress (dragging a value, a gesture on the canvas).
    group: Option<String>,
    /// Changes every time the document may have changed (to re-render).
    revision: u64,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub selection: Vec<ObjectId>,
}

impl Session {
    pub fn new(doc: Document, path: Option<PathBuf>) -> Session {
        Session {
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
            revision: 0,
            path,
            dirty: false,
            selection: Vec::new(),
        }
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Is this an untouched new document (that opening a file can replace silently)?
    pub fn is_pristine(&self) -> bool {
        self.path.is_none() && !self.dirty && self.doc.objects.is_empty()
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// One undoable change.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Document) -> R) -> R {
        self.group = None;
        self.checkpoint();
        self.revision += 1;
        f(&mut self.doc)
    }

    /// Changes with the same `key` until [`Self::end_group`] are undone together.
    pub fn edit_grouped<R>(&mut self, key: &str, f: impl FnOnce(&mut Document) -> R) -> R {
        if self.group.as_deref() != Some(key) {
            self.checkpoint();
            self.group = Some(key.to_string());
        }
        self.revision += 1;
        self.dirty = true;
        f(&mut self.doc)
    }

    pub fn end_group(&mut self) {
        self.group = None;
    }

    /// Throws away the grouped edit in progress (Esc during a gesture).
    pub fn cancel_group(&mut self) {
        if self.group.take().is_some()
            && let Some(prev) = self.undo.pop()
        {
            self.doc = prev;
            self.revision += 1;
            self.prune_selection();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.group = None;
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, prev));
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        self.group = None;
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, next));
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.revision += 1;
        self.dirty = true;
        self.prune_selection();
    }

    /// Drops selected ids that no longer exist.
    pub fn prune_selection(&mut self) {
        let doc = &self.doc;
        self.selection.retain(|id| doc.get(*id).is_some());
    }

    pub fn is_selected(&self, id: ObjectId) -> bool {
        self.selection.contains(&id)
    }

    pub fn select_only(&mut self, id: ObjectId) {
        self.selection.clear();
        self.selection.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Rgba;

    #[test]
    fn grouped_edits_undo_together() {
        let mut s = Session::new(Document::new(10, 10, Rgba::WHITE), None);
        s.edit_grouped("w", |d| d.width = 20);
        s.edit_grouped("w", |d| d.width = 30);
        s.end_group();
        s.edit(|d| d.width = 40);
        s.undo();
        assert_eq!(s.doc().width, 30);
        s.undo();
        assert_eq!(s.doc().width, 10);
        s.redo();
        assert_eq!(s.doc().width, 30);
        s.edit_grouped("h", |d| d.height = 5);
        s.cancel_group();
        assert_eq!(s.doc().height, 10);
    }
}
