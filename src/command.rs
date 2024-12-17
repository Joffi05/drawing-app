use serde::Deserialize;

use crate::Stroke;


pub enum Command {
    AddStroke(Stroke),
    RemoveStroke(Stroke),
}

pub struct CommandStack {
    undo_stack: Vec<Command>,
    redo_stack: Vec<Command>,
}

impl CommandStack {
    pub fn new() -> CommandStack {
        CommandStack {
            undo_stack: vec![],
            redo_stack: vec![],
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack = vec![];
        self.redo_stack = vec![];
    }

    pub fn push_undo(&mut self, comm: Command) {
        self.undo_stack.push(comm);
    }

    pub fn push_redo(&mut self, comm: Command) {
        self.redo_stack.push(comm);
    }

    pub fn pop_undo(&mut self) -> Option<Command> {
        self.undo_stack.pop()
    }

    pub fn pop_redo(&mut self) -> Option<Command> {
        self.redo_stack.pop()
    }
}