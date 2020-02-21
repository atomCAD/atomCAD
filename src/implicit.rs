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

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Id(usize);

impl Id {
    pub fn next() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static COUNTER: AtomicUsize = AtomicUsize::new(1);

        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

pub enum ParameterSettings {
    Scaler {
        bounds: (Option<f64>, Option<f64>),
    },
    Vec2,
    Vec3,
}

pub struct Parameter {
    pub name: String,
    pub description: String,
    pub settings: ParameterSettings,
    pub default_units: Units,
}

pub struct Description {
    pub name: String,
    pub description: String,
    pub parameters: Vec<Parameter>,
}

pub trait Implicit<JIT: Jit = crate::jit::glsl::GlslJit> {
    fn id(&self) -> Id;
    fn describe(&self) -> Description;
    fn compile(&self, jit: JIT, xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error>;
}