use std::fmt::Display;

pub mod glsl;

pub trait Variable: Clone {}

pub trait Error: Sized + std::error::Error {
    fn custom<T: Display>(msg: T) -> Self;
}

pub enum Swizzle {
    X,
    Y,
    Z,
}

pub trait Jit: Sized {
    type Ok;
    type Error: Error;
    type Variable: Variable;
    
    fn parameter(&mut self, name: &str) -> Result<Self::Variable, Self::Error>;
    fn constant(&mut self, value: f64) -> Result<Self::Variable, Self::Error>;
    fn make_vec2(&mut self, var0: Self::Variable, var1: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn make_vec3(&mut self, var0: Self::Variable, var1: Self::Variable, var2: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn swizzle(&mut self, vec: Self::Variable, swizzles: &[Swizzle]) -> Result<Self::Variable, Self::Error>;
    
    fn add(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn subtract(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn multiply(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn divide(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn power(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn negate(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn sqrt(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn abs(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;

    fn sin(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn cos(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn tan(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn atan2(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;

    fn max(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn min(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;

    fn length1(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error>;
    fn length2(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error>;

    fn clamp(&mut self, variable: Self::Variable, left_bound: Self::Variable, right_bound: Self::Variable) -> Result<Self::Variable, Self::Error>;

    fn end(self, distance: Self::Variable) -> Result<Self::Ok, Self::Error>;
}
