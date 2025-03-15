use std::any::Any;

// Provide a custom trait so that we can write a blanket implementation.
pub trait AsAny {
  fn as_any_ref(&self) -> &dyn Any;
  
  fn as_any_mut(&mut self) -> &mut dyn Any;
  
  fn as_any_box(self: Box<Self>) -> Box<dyn Any>;
}

// Write a blanket implementation for all types that implement `Any`.
impl<T> AsAny for T
where
  T: Any,
{
  // This cast cannot be written in a default implementation so cannot be
  // moved to the original trait without implementing it for every type.
  fn as_any_ref(&self) -> &dyn Any {
      self
  }

  fn as_any_mut(&mut self) -> &mut dyn Any {
      self
  }

  fn as_any_box(self: Box<Self>) -> Box<dyn Any> {
      self
  }
}