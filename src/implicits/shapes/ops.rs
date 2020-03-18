use crate::jit::Jit;
use crate::implicit::{Implicit, Units, Parameter, ParameterSettings, Description, Id};

pub struct Union {
    id: Id,
}

impl<JIT: Jit> Implicit<JIT> for Union {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }

    fn id(&self) -> &Id {
        &self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Union".into(),
            description: "This operation merges two implicit objects into one.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "shape 0".into(),
                    description: "The first shape to union.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
                Parameter::Custom {
                    name: "shape 1".into(),
                    description: "The second shape to union.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
            ],
            output_units: Units::Nanometer,
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.parameter("shape 0")?;
        let rhs = jit.parameter("shape 1")?;

        let distance = jit.min(lhs, rhs)?;

        jit.end(distance)
    }
}

pub struct SmoothUnion {
    id: Id,
}

impl<JIT: Jit> Implicit<JIT> for SmoothUnion {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }
    
    fn id(&self) -> &Id {
        &self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Smooth Union".into(),
            description: "This operation merges two implicit objects into one.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "shape 0".into(),
                    description: "The first shape to union.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
                Parameter::Custom {
                    name: "shape 1".into(),
                    description: "The second shape to union.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
                Parameter::Custom {
                    name: "r".into(),
                    description: "The radius of the smoothing effect.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                        units: Units::Nanometer,
                    },  
                },
            ],
            output_units: Units::Nanometer,
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        // let lhs = jit.compute(&*self.lhs)?;
        // let rhs = jit.compute(&*self.rhs)?;

        // let distance = jit.min(lhs, rhs)?;

        // jit.end(distance)
        unimplemented!()
    }
}

pub struct Subtraction {
    id: Id,
}

impl<JIT: Jit> Implicit<JIT> for Subtraction {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }
    
    fn id(&self) -> &Id {
        &self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Subtraction".into(),
            description: "This operation subtracts one implicit object from another.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "shape 0".into(),
                    description: "The primary shape.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
                Parameter::Custom {
                    name: "shape 1".into(),
                    description: "The shape to subtract from the primary shape.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
            ],
            output_units: Units::Nanometer,
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.parameter("shape 0")?;
        let rhs = jit.parameter("shape 1")?;
        
        let negated_lhs = jit.negate(lhs)?;

        let distance = jit.max(negated_lhs, rhs)?;

        jit.end(distance)
    }
}

pub struct Intersection {
    id: Id,
}

impl<JIT: Jit> Implicit<JIT> for Intersection {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }
    
    fn id(&self) -> &Id {
        &self.id
    }

    fn describe(&self) -> Description {
        Description {
            name: "Intersection".into(),
            description: "This operation finds the intersection of two implicit objects.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "shape 0".into(),
                    description: "The first shape to intersect.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
                Parameter::Custom {
                    name: "shape 1".into(),
                    description: "The second shape to intersect.".into(),
                    settings: ParameterSettings::Shape3D(Units::Nanometer),
                },
            ],
            output_units: Units::Nanometer,
        }
    }

    fn compile(&self, mut jit: JIT, _xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let lhs = jit.parameter("shape 0")?;
        let rhs = jit.parameter("shape 1")?;

        let distance = jit.max(lhs, rhs)?;

        jit.end(distance)
    }
}

pub struct Extrude {
    id: Id,
}

impl<JIT: Jit> Implicit<JIT> for Extrude {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }
    
    fn id(&self) -> &Id {
        &self.id
    }
    
    fn describe(&self) -> Description {
        Description {
            name: "Sphere".into(),
            description: "A sphere with radius `radius` positioned at `center`.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "shape".into(),
                    description: "The 2D shape to extrude.".into(),
                    settings: ParameterSettings::Shape2D(Units::Nanometer),
                },
                Parameter::Position(Units::Nanometer),
            ],
            output_units: Units::Nanometer,
        }
    }
    
    fn compile(&self, mut jit: JIT, from: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let radius = jit.parameter("shape")?;
        let center = jit.parameter("position")?;


        jit.end(distance_to_surface)
    }
}
