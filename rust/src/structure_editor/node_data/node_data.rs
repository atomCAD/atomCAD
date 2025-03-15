use crate::structure_editor::gadgets::gadget::Gadget;
use std::any::Any;
use crate::util::as_any::AsAny;

pub trait NodeData: std::fmt::Debug + Any + AsAny  {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>>;
}
