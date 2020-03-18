use crate::jit::Jit;
use crate::implicit::{Implicit, Units, Parameter, ParameterSettings, Description, Id};

pub struct Cuboid {
    id: Id,
}

impl<JIT: Jit> Implicit<JIT> for Cuboid {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }
    
    fn id(&self) -> &Id {
        &self.id
    }
    
    fn describe(&self) -> Description {
        Description {
            name: "Cuboid".into(),
            description: "A cuboid with length, width, and height positioned at `center`.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "length".into(),
                    description: "The length of the cuboid.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                        units: Units::Nanometer,
                    },
                },
                Parameter::Custom {
                    name: "width".into(),
                    description: "The width of the cuboid.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                        units: Units::Nanometer,
                    },
                },
                Parameter::Custom {
                    name: "height".into(),
                    description: "The height of the cuboid.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                        units: Units::Nanometer,
                    },
                },
                Parameter::Position(Units::Nanometer),
            ],
            output_units: Units::Nanometer,
        }
    }
    
    fn compile(&self, mut jit: JIT, xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let length = jit.parameter("length")?;
        let width = jit.parameter("width")?;
        let height = jit.parameter("height")?;
        let position = jit.parameter("position")?;
        
        let d = jit.length2(position, xyz)?;
        

        // jit.end(...)
        unimplemented!()
    }
}
