use crate::implicit::{Implicit, Description, Id, Parameter, ParameterSettings, Units};
use crate::jit;

pub mod sphere;
pub mod boxes;

pub struct Union {
    id: Id,
    lhs: Box<dyn Implicit>,
    rhs: Box<dyn Implicit>,
}

impl<JIT: jit::Jit> Implicit<JIT> for Union {
    fn id(&self) -> Id {
        self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Union".into(),
            description: "This operation merges two implicit objects into one.".into(),
            parameters: vec![],
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.compute(&*self.lhs)?;
        let rhs = jit.compute(&*self.rhs)?;

        let distance = jit.min(lhs, rhs)?;

        jit.end(distance)
    }
}

pub struct SmoothUnion {
    id: Id,
    lhs: Box<dyn Implicit>,
    rhs: Box<dyn Implicit>,
}

impl<JIT: jit::Jit> Implicit<JIT> for SmoothUnion {
    fn id(&self) -> Id {
        self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Smooth Union".into(),
            description: "This operation merges two implicit objects into one.".into(),
            parameters: vec![
                Parameter {
                    name: "blend".into(),
                    description: "The size (in units) of the blending operation..".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                    },
                    default_units: Units::Nanometer,
                },
            ],
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.compute(&*self.lhs)?;
        let rhs = jit.compute(&*self.rhs)?;

        let distance = jit.min(lhs, rhs)?;

        jit.end(distance)
    }
}

pub struct Subtraction {
    id: Id,
    lhs: Box<dyn Implicit>,
    rhs: Box<dyn Implicit>,
}

impl<JIT: jit::Jit> Implicit<JIT> for Subtraction {
    fn id(&self) -> Id {
        self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Subtraction".into(),
            description: "This operation subtracts one implicit object from another.".into(),
            parameters: vec![],
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.compute(&*self.lhs)?;
        let rhs = jit.compute(&*self.rhs)?;
        
        let negated_lhs = jit.negate(lhs)?;

        let distance = jit.max(negated_lhs, rhs)?;

        jit.end(distance)
    }
}

pub struct Intersection {
    id: Id,
    lhs: Box<dyn Implicit>,
    rhs: Box<dyn Implicit>,
}

impl<JIT: jit::Jit> Implicit<JIT> for Intersection {
    fn id(&self) -> Id {
        self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Intersection".into(),
            description: "This operation finds the intersection of two implicit objects.".into(),
            parameters: vec![],
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.compute(&*self.lhs)?;
        let rhs = jit.compute(&*self.rhs)?;

        let distance = jit.max(lhs, rhs)?;

        jit.end(distance)
    }
}