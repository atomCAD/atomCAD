use crate::jit::Jit;
use crate::implicit::{Implicit2D, Units, Parameter, ParameterSettings, Description, Id};

pub struct Circle {
    id: Id,
}

impl<JIT: Jit> Implicit2D<JIT> for Circle {
    fn new_with_id(id: Id) -> Self {
        Self { id }
    }
    
    fn id(&self) -> &Id {
        &self.id
    }
    
    fn describe(&self) -> Description {
        Description {
            name: "Circle".into(),
            description: "A circle with radius `radius` positioned at `center`.".into(),
            parameters: vec![
                Parameter::Custom {
                    name: "radius".into(),
                    description: "The radius of the circle.".into(),
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
    
    fn compile(&self, mut jit: JIT, from: JIT::Variable) -> Result<JIT::Ok, JIT::Error> {
        let radius = jit.parameter("radius")?;
        let center = jit.parameter("position")?;

        let distance_to_center = jit.length2(center, from)?;
        let distance_to_surface = jit.subtract(distance_to_center, radius)?;

        jit.end(distance_to_surface)
    }
}
