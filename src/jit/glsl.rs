use std::fmt::Display;
use crate::{jit, implicit::Implicit};

pub struct GlslJit {

}

impl jit::Jit for GlslJit {
    type Ok = Ok;
    type Error = Error;
    type Variable = Variable;
    
    fn parameter(&mut self, name: &str) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn constant(&mut self, value: f64) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn make_vec2(&mut self, var0: Self::Variable, var1: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn make_vec3(&mut self, var0: Self::Variable, var1: Self::Variable, var2: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    
    fn add(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn subtract(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }
    fn multiply(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn divide(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn power(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn negate(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn sqrt(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn abs(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }


    fn sin(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn cos(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn tan(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn atan2(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }


    fn max(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn min(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }


    fn length1(&mut self, variable: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn length2(&mut self, lhs: Self::Variable, rhs: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }


    fn clamp(&mut self, variable: Self::Variable, left_bound: Self::Variable, right_bound: Self::Variable) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }


    fn compute(&mut self, implicit: &dyn Implicit) -> Result<Self::Variable, Self::Error> {
        unimplemented!()
    }

    fn end(self, distance: Self::Variable) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }
}

#[derive(Clone)]
pub struct Variable {
    
}

impl jit::Variable for Variable {}

pub struct Ok;

#[derive(Debug)]
pub struct Error {
    msg: String,
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for Error {}

impl jit::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error {
            msg: msg.to_string(),
        }
    }
}