use crate::kernel::gadgets::gadget::Gadget;
use std::any::Any;
use crate::kernel::as_any::AsAny;

pub trait NodeData: std::fmt::Debug + Any + AsAny  {
    fn provide_gadget(&self) -> Option<Box<dyn Gadget>>;
}
