use crate::jit::Jit;
use crate::implicit::{Implicit, Point, Units, Parameter, ParameterSettings, Description, Id};

pub struct Box {
    id: Id,
    pub pos: Point,
    pub length: f64,
    pub width: f64,
    pub height: f64,
}

impl<JIT: Jit> Implicit<JIT> for Box {
    fn id(&self) -> Id {
        self.id
    }
    
    fn describe(&self) -> Description {
        Description {
            name: "Box".into(),
            description: "A box with length, width, and height positioned at `center`.".into(),
            parameters: vec![
                Parameter {
                    name: "length".into(),
                    description: "The length of the box.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                    },
                    default_units: Units::Nanometer,
                },
                Parameter {
                    name: "width".into(),
                    description: "The width of the box.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                    },
                    default_units: Units::Nanometer,
                },
                Parameter {
                    name: "height".into(),
                    description: "The height of the box.".into(),
                    settings: ParameterSettings::Scaler {
                        bounds: (Some(0.0), None),
                    },
                    default_units: Units::Nanometer,
                },
                Parameter {
                    name: "position".into(),
                    description: "The location where the box is centered.".into(),
                    settings: ParameterSettings::Vec3,
                    default_units: Units::Nanometer,
                },
            ],
        }
    }
    
    fn compile(&self, mut jit: JIT, xyz: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let length = jit.parameter("length")?;
        let width = jit.parameter("width")?;
        let height = jit.parameter("height")?;
        let position = jit.parameter("position")?;
        


        jit.end(...)
    }
}
