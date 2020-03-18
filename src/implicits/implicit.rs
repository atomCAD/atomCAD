use crate::jit::Jit;

pub struct Point {
    x: f64,
    y: f64,
    z: f64,
}

pub enum Units {
    Angstrom,
    Nanometer,
    Picometer,
    None,
}

#[derive(Eq, PartialEq, Hash)]
pub struct Id(usize);

impl Id {
    fn next() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static COUNTER: AtomicUsize = AtomicUsize::new(1);

        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

pub enum ParameterSettings {
    Scaler {
        bounds: (Option<f64>, Option<f64>),
        units: Units,
    },
    Shape2D(Units),
    Shape3D(Units),
}

pub enum Parameter {
    Custom {
        name: String,
        description: String,
        settings: ParameterSettings,
    },
    Position(Units),
}

pub struct Description {
    pub name: String,
    pub description: String,
    pub parameters: Vec<Parameter>,
    pub output_units: Units,
}

pub trait Implicit<JIT: Jit = crate::jit::glsl::GlslJit>: Sized {
    fn new() -> Self {
        Self::new_with_id(Id::next())
    }
    fn new_with_id(id: Id) -> Self;
    fn id(&self) -> &Id;
    fn describe(&self) -> Description;
    fn compile(&self, jit: JIT, from: JIT::Variable) -> Result<JIT::Ok, JIT::Error>;
}

pub trait Implicit2D<JIT: Jit = crate::jit::glsl::GlslJit>: Sized {
    fn new() -> Self {
        Self::new_with_id(Id::next())
    }
    fn new_with_id(id: Id) -> Self;
    fn id(&self) -> &Id;
    fn describe(&self) -> Description;
    fn compile(&self, jit: JIT, from: JIT::Variable) -> Result<JIT::Ok, JIT::Error>;
}