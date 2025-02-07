use super::model::Model;
use super::as_any::AsAny;

pub trait Command : AsAny { 
  fn execute(&mut self, model: &mut Model, is_redo: bool);
  fn undo(&mut self, model: &mut Model);
}
